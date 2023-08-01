use std::{marker::PhantomData, path::PathBuf};

use colored::Colorize;
use futures::{
    future::{self, LocalBoxFuture},
    select,
    stream::{self, LocalBoxStream},
    FutureExt, SinkExt, StreamExt, TryStreamExt,
};
use notify::{recommended_watcher, Event, EventKind, RecursiveMode, Watcher};
use reactive::driver::{
    ChildProcessKillerDriver, Driver, EprintlnDriver, StaticCommandDriver, StaticOpenThatDriver,
};
use thiserror::Error;
use tokio::process::{Child, Command};
use url::Url;

const NEVER_ENDING_STREAM: &str = "never ending stream";

#[derive(Debug, Error)]
#[allow(clippy::module_name_repetitions)]
pub enum DevError {
    #[error(transparent)]
    FsWatch(notify::Error),
    #[error(transparent)]
    Io(std::io::Error),
    #[error("no free port")]
    NoFreePort,
}

const BUILDER_CRATE_NAME: &str = "builder";
const LOCALHOST: &str = "localhost";

struct FsChangeDriver<Path>(
    futures::channel::mpsc::Sender<notify::Result<notify::Event>>,
    Path,
);

impl<T: 'static> Driver for FsChangeDriver<T>
where
    std::path::PathBuf: From<T>,
{
    type Init = T;
    type Input = ();
    type Output = LocalBoxStream<'static, notify::Result<()>>;

    fn new(_init: Self::Init) -> (Self, Self::Output) {
        let (sender, receiver) = futures::channel::mpsc::channel::<notify::Result<Event>>(1);

        let receiver = receiver
            .try_filter_map(|event| async move {
                Ok(match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => Some(()),
                    _ => None,
                })
            })
            .boxed_local();

        (Self(sender), receiver)
    }

    fn init(mut self, _input: Self::Input) -> LocalBoxFuture<'static, ()> {
        let mut sender = self.0.clone();

        let watcher = recommended_watcher(move |result: Result<Event, notify::Error>| {
            futures::executor::block_on(sender.feed(result))
                .expect("this closure gets sent to a blocking context");
        });

        let mut watcher = match watcher {
            Ok(watcher) => watcher,
            Err(error) => {
                futures::executor::block_on(self.0.feed(Err(error))).unwrap();
                return futures::future::pending().boxed_local();
            }
        };

        if let Err(error) = watcher.watch(
            &std::path::PathBuf::from(BUILDER_CRATE_NAME),
            RecursiveMode::Recursive,
        ) {
            futures::executor::block_on(self.0.feed(Err(error))).unwrap();
            return futures::future::pending().boxed_local();
        };

        futures::future::pending().boxed_local()
    }
}

#[allow(clippy::missing_panics_doc)]
pub async fn dev<O: AsRef<camino::Utf8Path>>(launch_browser: bool, output_dir: O) -> DevError {
    let output_dir = output_dir.as_ref().to_owned();
    let Some(port) = portpicker::pick_unused_port() else { return DevError::NoFreePort };

    let server_task = live_server::listen(LOCALHOST, port, output_dir.as_std_path().to_owned())
        .map(|result| result.expect_err("unreachable"))
        .boxed();

    let mut cargo_run_builder = Command::new("cargo");
    cargo_run_builder.args(["run", "--package", BUILDER_CRATE_NAME]);

    let (builder_driver, builder_started) = StaticCommandDriver::new(cargo_run_builder);
    let (child_process_killer_driver, child_killed) = ChildProcessKillerDriver::new(());
    let url = Url::parse(&format!("http://{LOCALHOST}:{port}")).unwrap();
    let (open_url_driver, browser_launch) = StaticOpenThatDriver::new(url.to_string());
    let (eprintln_driver, _) = EprintlnDriver::new(());
    let (fs_change_driver, fs_change) = FsChangeDriver::new(());

    let inputs = Inputs {
        server_task,
        child_killed,
        fs_change,
        builder_started,
        launch_browser,
        open_that_driver: browser_launch,
        local_host_port_url: url,
    };

    let outputs = app(inputs);

    let Outputs {
        stderr,
        open_browser: launch_browser,
        error: app_error,
        kill_child,
        run_builder,
        some_task,
    } = outputs;

    let builder_driver_task = builder_driver.init(run_builder);
    let child_killer_task = child_process_killer_driver.init(kill_child);
    let open_url_driver_task = open_url_driver.init(launch_browser);
    let stderr_driver_task = eprintln_driver.init(stderr);
    let fs_change_driver_task = fs_change_driver.init(());

    let app_error = select! {
        error = app_error.fuse() => error,
        _ = builder_driver_task.fuse() => unreachable!(),
        _ = child_killer_task.fuse() => unreachable!(),
        _ = stderr_driver_task.fuse() => unreachable!(),
        _ = open_url_driver_task.fuse() => unreachable!(),
        _ = some_task.fuse() => unreachable!(),
        _ = fs_change_driver_task.fuse() => unreachable!(),
    };

    app_error
}

enum InputEvent {
    BuilderKilled(Result<(), std::io::Error>),
    BuilderCrateFsChange(Result<(), notify::Error>),
    BuilderStarted(Result<Child, std::io::Error>),
    BrowserLaunched(Result<(), std::io::Error>),
    ServerError(std::io::Error),
}

#[derive(Debug)]
enum OutputEvent {
    Stderr(String),
    RunBuilder,
    KillChild(Child),
    Error(DevError),
    OpenBrowser,
}

struct Inputs {
    server_task: LocalBoxFuture<'static, std::io::Error>,
    child_killed: LocalBoxStream<'static, Result<(), std::io::Error>>,
    fs_change: LocalBoxStream<'static, Result<(), notify::Error>>,
    builder_started: LocalBoxStream<'static, Result<Child, std::io::Error>>,
    launch_browser: bool,
    open_that_driver: LocalBoxStream<'static, Result<(), std::io::Error>>,
    local_host_port_url: Url,
}

struct Outputs {
    stderr: LocalBoxStream<'static, String>,
    kill_child: LocalBoxStream<'static, Child>,
    run_builder: LocalBoxStream<'static, ()>,
    open_browser: LocalBoxStream<'static, ()>,
    error: LocalBoxFuture<'static, DevError>,
    some_task: LocalBoxFuture<'static, ()>,
}

#[derive(Debug, Default)]
struct State {
    builder: BuilderState,
}

#[derive(Debug, Default)]
enum BuilderState {
    None,
    Obsolete,
    #[default]
    Starting,
    Started(Child),
}

impl BuilderState {
    fn killing(&mut self) -> Option<Child> {
        if let Self::Started(child) = std::mem::replace(self, Self::None) {
            Some(child)
        } else {
            None
        }
    }
}

// TODO too many lines
#[allow(clippy::too_many_lines)]
fn app(inputs: Inputs) -> Outputs {
    let Inputs {
        server_task,
        child_killed,
        fs_change: builder_crate_fs_change,
        builder_started,
        launch_browser,
        open_that_driver: browser_launch,
        local_host_port_url,
    } = inputs;

    let message = format!("\nServer started at {local_host_port_url}\n")
        .blue()
        .to_string();

    let mut initial = vec![OutputEvent::RunBuilder, OutputEvent::Stderr(message)];
    if launch_browser {
        initial.push(OutputEvent::OpenBrowser);
    }
    let initial = stream::iter(initial);

    let reaction = stream::select_all([
        stream::once(server_task)
            .map(InputEvent::ServerError)
            .boxed_local(),
        child_killed.map(InputEvent::BuilderKilled).boxed_local(),
        builder_crate_fs_change
            .map(InputEvent::BuilderCrateFsChange)
            .boxed_local(),
        builder_started
            .map(InputEvent::BuilderStarted)
            .boxed_local(),
        browser_launch
            .map(InputEvent::BrowserLaunched)
            .boxed_local(),
    ])
    .scan(State::default(), move |state, input| {
        let emit = match input {
            InputEvent::BuilderKilled(result) => match result {
                Ok(_) => {
                    state.builder = BuilderState::None;
                    Some(OutputEvent::RunBuilder)
                }
                Err(error) => Some(OutputEvent::Error(DevError::Io(error))),
            },
            InputEvent::BuilderCrateFsChange(result) => match result {
                Ok(_) => match &mut state.builder {
                    BuilderState::Starting => {
                        state.builder = BuilderState::Obsolete;
                        None
                    }
                    BuilderState::Started(_) => {
                        let child = state.builder.killing().unwrap();
                        Some(OutputEvent::KillChild(child))
                    }
                    BuilderState::None | BuilderState::Obsolete => None,
                },
                Err(error) => Some(OutputEvent::Error(DevError::FsWatch(error))),
            },
            InputEvent::BuilderStarted(child) => match child {
                Ok(child) => match state.builder {
                    BuilderState::None | BuilderState::Starting => {
                        state.builder = BuilderState::Started(child);
                        None
                    }
                    BuilderState::Obsolete => {
                        state.builder = BuilderState::None;
                        Some(OutputEvent::KillChild(child))
                    }
                    BuilderState::Started(_) => {
                        // TODO is this reachable?
                        let current_child = state.builder.killing().unwrap();
                        state.builder = BuilderState::Started(child);
                        Some(OutputEvent::KillChild(current_child))
                    }
                },
                Err(error) => Some(OutputEvent::Error(DevError::Io(error))),
            },
            InputEvent::BrowserLaunched(result) => match result {
                Ok(_) => None,
                Err(error) => Some(OutputEvent::Error(DevError::Io(error))),
            },
            InputEvent::ServerError(error) => Some(OutputEvent::Error(DevError::Io(error))),
        };

        future::ready(Some(emit))
    })
    .filter_map(future::ready);

    let output = initial.chain(reaction);

    let (kill_child_sender, kill_child) = futures::channel::mpsc::channel(1);
    let (run_builder_sender, run_builder) = futures::channel::mpsc::channel(1);
    let (error_sender, error) = futures::channel::mpsc::channel(1);
    let (stderr_sender, stderr) = futures::channel::mpsc::channel(1);
    let (open_browser_sender, open_browser) = futures::channel::mpsc::channel(1);

    let error = error
        .into_future()
        .map(|(error, _tail_of_stream)| error.expect(NEVER_ENDING_STREAM))
        .boxed_local();

    let some_task = output
        .for_each(move |event| match event {
            OutputEvent::RunBuilder => {
                let mut sender_clone = run_builder_sender.clone();
                async move {
                    sender_clone.send(()).await.expect(NEVER_ENDING_STREAM);
                }
                .boxed_local()
            }
            OutputEvent::KillChild(child) => {
                let mut sender_clone = kill_child_sender.clone();
                async move {
                    sender_clone.send(child).await.expect(NEVER_ENDING_STREAM);
                }
                .boxed_local()
            }
            OutputEvent::Error(error) => {
                let mut sender_clone = error_sender.clone();
                async move {
                    sender_clone.send(error).await.expect(NEVER_ENDING_STREAM);
                }
                .boxed_local()
            }
            OutputEvent::Stderr(output) => {
                let mut sender_clone = stderr_sender.clone();
                async move {
                    sender_clone.send(output).await.expect(NEVER_ENDING_STREAM);
                }
                .boxed_local()
            }
            OutputEvent::OpenBrowser => {
                let mut sender_clone = open_browser_sender.clone();
                async move {
                    sender_clone.send(()).await.expect(NEVER_ENDING_STREAM);
                }
                .boxed_local()
            }
        })
        .boxed_local();

    Outputs {
        stderr: stderr.boxed_local(),
        kill_child: kill_child.boxed_local(),
        run_builder: run_builder.boxed_local(),
        open_browser: open_browser.boxed_local(),
        error,
        some_task,
    }
}
