use colored::Colorize;
use futures::{FutureExt, SinkExt, StreamExt};
use reactive::driver::Driver;

const NEVER_ENDING_STREAM: &str = "never ending stream";

#[derive(Debug, thiserror::Error)]
#[allow(clippy::module_name_repetitions)]
pub enum DevError {
    #[error(transparent)]
    FsWatch(reactive::driver::fs_change::Error),
    #[error(transparent)]
    Io(std::io::Error),
    #[error("no free port")]
    NoFreePort,
}

const BUILDER_CRATE_NAME: &str = "builder";
const LOCALHOST: &str = "localhost";

#[allow(clippy::missing_panics_doc)]
pub async fn dev<O: AsRef<camino::Utf8Path>>(launch_browser: bool, output_dir: O) -> DevError {
    let output_dir = output_dir.as_ref().to_owned();
    let Some(port) = portpicker::pick_unused_port() else { return DevError::NoFreePort };

    let server_task = live_server::listen(LOCALHOST, port, output_dir.as_std_path().to_owned())
        .map(|result| result.expect_err("unreachable"))
        .boxed();

    let mut cargo_run_builder = tokio::process::Command::new("cargo");
    cargo_run_builder.args(["run", "--package", BUILDER_CRATE_NAME]);

    let (builder_driver, builder_started) =
        reactive::driver::command::StaticCommandDriver::new(cargo_run_builder);
    let (child_process_killer_driver, child_killed) =
        reactive::driver::child_process_killer::ChildProcessKillerDriver::new(());
    let url = reqwest::Url::parse(&format!("http://{LOCALHOST}:{port}")).unwrap();
    let (open_url_driver, browser_launch) =
        reactive::driver::open_that::StaticOpenThatDriver::new(url.to_string());
    let (eprintln_driver, _) = reactive::driver::eprintln::EprintlnDriver::new(());
    let (fs_change_driver, fs_change) =
        reactive::driver::fs_change::FsChangeDriver::new(BUILDER_CRATE_NAME);

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

    let app_error = futures::select! {
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
    FsChange(reactive::driver::fs_change::Result<reactive::driver::fs_change::Event>),
    BuilderStarted(Result<tokio::process::Child, std::io::Error>),
    BrowserLaunched(Result<(), std::io::Error>),
    ServerError(std::io::Error),
}

#[derive(Debug)]
enum OutputEvent {
    Stderr(String),
    RunBuilder,
    KillChild(tokio::process::Child),
    Error(DevError),
    OpenBrowser,
}

struct Inputs {
    server_task: futures::future::LocalBoxFuture<'static, std::io::Error>,
    child_killed: futures::stream::LocalBoxStream<'static, Result<(), std::io::Error>>,
    fs_change: futures::stream::LocalBoxStream<
        'static,
        reactive::driver::fs_change::Result<reactive::driver::fs_change::Event>,
    >,
    builder_started:
        futures::stream::LocalBoxStream<'static, Result<tokio::process::Child, std::io::Error>>,
    launch_browser: bool,
    open_that_driver: futures::stream::LocalBoxStream<'static, Result<(), std::io::Error>>,
    local_host_port_url: reqwest::Url,
}

struct Outputs {
    stderr: futures::stream::LocalBoxStream<'static, String>,
    kill_child: futures::stream::LocalBoxStream<'static, tokio::process::Child>,
    run_builder: futures::stream::LocalBoxStream<'static, ()>,
    open_browser: futures::stream::LocalBoxStream<'static, ()>,
    error: futures::future::LocalBoxFuture<'static, DevError>,
    some_task: futures::future::LocalBoxFuture<'static, ()>,
}

#[derive(Debug, Default)]
struct State {
    builder: BuilderState,
}

impl State {
    #[allow(clippy::unnecessary_wraps)]
    fn builder_killed(&mut self, result: Result<(), std::io::Error>) -> Option<OutputEvent> {
        match result {
            Ok(_) => {
                self.builder = BuilderState::None;
                Some(OutputEvent::RunBuilder)
            }
            Err(error) => Some(OutputEvent::Error(DevError::Io(error))),
        }
    }

    fn fs_change(
        &mut self,
        result: Result<reactive::driver::fs_change::Event, reactive::driver::fs_change::Error>,
    ) -> Option<OutputEvent> {
        match result {
            Ok(event) => match event.kind {
                reactive::driver::fs_change::EventKind::Create(_)
                | reactive::driver::fs_change::EventKind::Modify(_)
                | reactive::driver::fs_change::EventKind::Remove(_) => match &mut self.builder {
                    BuilderState::Starting => {
                        self.builder = BuilderState::Obsolete;
                        None
                    }
                    BuilderState::Started(_) => {
                        let child = self.builder.killing().unwrap();
                        Some(OutputEvent::KillChild(child))
                    }
                    BuilderState::None | BuilderState::Obsolete => None,
                },
                _ => None,
            },
            Err(error) => Some(OutputEvent::Error(DevError::FsWatch(error))),
        }
    }

    fn builder_started(
        &mut self,
        child: Result<tokio::process::Child, std::io::Error>,
    ) -> Option<OutputEvent> {
        match child {
            Ok(child) => match self.builder {
                BuilderState::None | BuilderState::Starting => {
                    self.builder = BuilderState::Started(child);
                    None
                }
                BuilderState::Obsolete => {
                    self.builder = BuilderState::None;
                    Some(OutputEvent::KillChild(child))
                }
                BuilderState::Started(_) => {
                    // TODO is this reachable?
                    let current_child = self.builder.killing().unwrap();
                    self.builder = BuilderState::Started(child);
                    Some(OutputEvent::KillChild(current_child))
                }
            },
            Err(error) => Some(OutputEvent::Error(DevError::Io(error))),
        }
    }
}

#[derive(Debug, Default)]
enum BuilderState {
    None,
    Obsolete,
    #[default]
    Starting,
    Started(tokio::process::Child),
}

impl BuilderState {
    fn killing(&mut self) -> Option<tokio::process::Child> {
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
    let initial = futures::stream::iter(initial);

    let reaction = futures::stream::select_all([
        futures::stream::once(server_task)
            .map(InputEvent::ServerError)
            .boxed_local(),
        child_killed.map(InputEvent::BuilderKilled).boxed_local(),
        builder_crate_fs_change
            .map(InputEvent::FsChange)
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
            InputEvent::BuilderKilled(result) => state.builder_killed(result),
            InputEvent::FsChange(result) => state.fs_change(result),
            InputEvent::BuilderStarted(child) => state.builder_started(child),
            InputEvent::BrowserLaunched(result) => match result {
                Ok(_) => None,
                Err(error) => Some(OutputEvent::Error(DevError::Io(error))),
            },
            InputEvent::ServerError(error) => Some(OutputEvent::Error(DevError::Io(error))),
        };

        futures::future::ready(Some(emit))
    })
    .filter_map(futures::future::ready);

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
