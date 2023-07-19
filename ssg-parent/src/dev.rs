use std::path::PathBuf;

use async_fn_stream::{fn_stream, try_fn_stream};
use camino::Utf8Path;
use colored::Colorize;
use future_handles::sync::CompleteHandle;
use futures::{
    future::{self, LocalBoxFuture},
    select,
    stream::{self, LocalBoxStream},
    FutureExt, StreamExt, TryStreamExt,
};
use notify::{recommended_watcher, Event, EventKind, RecursiveMode, Watcher};
use portpicker::Port;
use reactive::driver::{eprintln::EprintlnDriver, Driver};
use thiserror::Error;
use tokio::{
    process::{Child, Command},
    sync::mpsc,
};
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

#[allow(clippy::missing_panics_doc)]
pub async fn dev<O: AsRef<Utf8Path>>(launch_browser: bool, output_dir: O) -> DevError {
    let output_dir = output_dir.as_ref().to_owned();
    let Some(port) = portpicker::pick_unused_port() else { return DevError::NoFreePort };

    let server_task = live_server::listen(LOCALHOST, port, output_dir.as_std_path().to_owned())
        .map(|result| result.expect_err("unreachable"))
        .boxed();

    let builder_crate_fs_change = try_fn_stream(|emitter| async move {
        let (sender, mut receiver) = mpsc::channel(1);

        let mut watcher = recommended_watcher(move |result: Result<Event, notify::Error>| {
            sender
                .blocking_send(result)
                .expect("this closure gets sent to a blocking context");
        })?;

        watcher.watch(&PathBuf::from(BUILDER_CRATE_NAME), RecursiveMode::Recursive)?;

        loop {
            let event = receiver.recv().await.unwrap()?;
            emitter.emit(event).await;
        }
    })
    .try_filter_map(|event| async move {
        if let EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) = event.kind {
            Ok(Some(()))
        } else {
            Ok(None)
        }
    })
    .boxed();

    let (builder_driver, builder_started) = BuilderDriver::new(());
    let (child_killer_driver, child_killed) = ChildKillerDriver::new(());
    let (browser_launch_driver, browser_launch) = BrowserLaunchDriver::new(());
    let (eprintln_driver, _) = EprintlnDriver::new(());

    let inputs = Inputs {
        server_task,
        port,
        child_killed,
        builder_crate_fs_change,
        builder_started,
        launch_browser,
        browser_launch,
    };

    let outputs = app(inputs);

    let Outputs {
        stderr,
        launch_browser,
        error: app_error,
        kill_child,
        run_builder,
        some_task,
    } = outputs;

    let builder_driver_task = builder_driver.init(run_builder);
    let child_killer_task = child_killer_driver.init(kill_child);
    let browser_launcher_driver_task = browser_launch_driver.init(launch_browser);
    let stderr_driver_task = eprintln_driver.init(stderr);

    let app_error = select! {
        error = app_error.fuse() => error,
        _ = builder_driver_task.fuse() => unreachable!(),
        _ = child_killer_task.fuse() => unreachable!(),
        _ = stderr_driver_task.fuse() => unreachable!(),
        _ = browser_launcher_driver_task.fuse() => unreachable!(),
        _ = some_task.fuse() => unreachable!(),
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
}

struct Inputs {
    server_task: LocalBoxFuture<'static, std::io::Error>,
    port: Port,
    child_killed: LocalBoxStream<'static, Result<(), std::io::Error>>,
    builder_crate_fs_change: LocalBoxStream<'static, Result<(), notify::Error>>,
    builder_started: LocalBoxStream<'static, Result<Child, std::io::Error>>,
    launch_browser: bool,
    browser_launch: LocalBoxFuture<'static, Result<(), std::io::Error>>,
}

struct Outputs {
    stderr: LocalBoxStream<'static, String>,
    kill_child: LocalBoxStream<'static, Child>,
    run_builder: LocalBoxStream<'static, ()>,
    launch_browser: LocalBoxFuture<'static, Url>,
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
        port,
        child_killed,
        builder_crate_fs_change,
        builder_started,
        launch_browser,
        browser_launch,
    } = inputs;

    let url = Url::parse(&format!("http://{LOCALHOST}:{port}")).unwrap();
    let message = format!("\nServer started at {url}\n").blue().to_string();

    let initial = stream::iter([OutputEvent::RunBuilder, OutputEvent::Stderr(message)]);

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
        stream::once(browser_launch)
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

    let (kill_child_sender, mut kill_child_receiver) = mpsc::channel(1);
    let (run_builder_sender, mut start_builder_receiver) = mpsc::channel(1);
    let (error_sender, mut error_receiver) = mpsc::channel(1);
    let (stderr_sender, mut stderr_receiver) = mpsc::channel(1);

    let kill_child = fn_stream(|emitter| async move {
        loop {
            let value = kill_child_receiver.recv().await.expect(NEVER_ENDING_STREAM);
            emitter.emit(value).await;
        }
    })
    .boxed_local();

    let run_builder = fn_stream(|emitter| async move {
        loop {
            start_builder_receiver
                .recv()
                .await
                .expect(NEVER_ENDING_STREAM);
            emitter.emit(()).await;
        }
    })
    .boxed_local();

    let error = fn_stream(|emitter| async move {
        loop {
            let value = error_receiver.recv().await.expect(NEVER_ENDING_STREAM);
            emitter.emit(value).await;
        }
    })
    .boxed_local()
    .into_future()
    .map(|(error, _tail_of_stream)| error.expect(NEVER_ENDING_STREAM))
    .boxed_local();

    let stderr = fn_stream(|emitter| async move {
        loop {
            let value = stderr_receiver.recv().await.expect(NEVER_ENDING_STREAM);
            emitter.emit(value).await;
        }
    })
    .boxed_local();

    let some_task = output
        .for_each(move |event| match event {
            OutputEvent::RunBuilder => {
                let sender_clone = run_builder_sender.clone();
                async move {
                    sender_clone.send(()).await.expect(NEVER_ENDING_STREAM);
                }
                .boxed_local()
            }
            OutputEvent::KillChild(child) => {
                let sender_clone = kill_child_sender.clone();
                async move {
                    sender_clone.send(child).await.expect(NEVER_ENDING_STREAM);
                }
                .boxed_local()
            }
            OutputEvent::Error(error) => {
                let sender_clone = error_sender.clone();
                async move {
                    sender_clone.send(error).await.expect(NEVER_ENDING_STREAM);
                }
                .boxed_local()
            }
            OutputEvent::Stderr(output) => {
                let sender_clone = stderr_sender.clone();
                async move {
                    sender_clone.send(output).await.expect(NEVER_ENDING_STREAM);
                }
                .boxed_local()
            }
        })
        .boxed_local();

    let launch_browser = if launch_browser {
        future::ready(url).boxed_local()
    } else {
        future::pending().boxed_local()
    };

    Outputs {
        stderr,
        kill_child,
        run_builder,
        launch_browser,
        error,
        some_task,
    }
}

#[derive(Debug)]
struct BuilderDriver(mpsc::Sender<Result<Child, std::io::Error>>);

impl BuilderDriver {
    fn cargo_run_builder() -> Result<Child, std::io::Error> {
        Command::new("cargo")
            .args(["run", "--package", BUILDER_CRATE_NAME])
            .spawn()
    }
}

impl Driver for BuilderDriver {
    type Init = ();
    type Input = LocalBoxStream<'static, ()>;
    type Output = LocalBoxStream<'static, Result<Child, std::io::Error>>;

    fn new(_init: Self::Init) -> (Self, Self::Output) {
        let (sender, mut receiver) = mpsc::channel(1);
        let stream = fn_stream(|emitter| async move {
            loop {
                let input = receiver.recv().await.unwrap();
                emitter.emit(input).await;
            }
        })
        .boxed();

        let builder_driver = Self(sender);

        (builder_driver, stream)
    }

    fn init(self, mut start_builder: Self::Input) -> LocalBoxFuture<'static, ()> {
        async move {
            loop {
                start_builder.next().await.unwrap();
                let child = Self::cargo_run_builder();
                self.0.send(child).await.unwrap();
            }
        }
        .boxed_local()
    }
}

struct ChildKillerDriver(mpsc::Sender<Result<(), std::io::Error>>);

impl Driver for ChildKillerDriver {
    type Init = ();
    type Input = LocalBoxStream<'static, Child>;
    type Output = LocalBoxStream<'static, Result<(), std::io::Error>>;

    fn new(_init: Self::Init) -> (Self, Self::Output) {
        let (sender, mut receiver) = mpsc::channel(1);

        let stream = fn_stream(|emitter| async move {
            loop {
                let value = receiver.recv().await.expect(NEVER_ENDING_STREAM);
                emitter.emit(value).await;
            }
        })
        .boxed();

        let child_killer_driver = Self(sender);

        (child_killer_driver, stream)
    }

    fn init(self, mut kill_child: Self::Input) -> LocalBoxFuture<'static, ()> {
        async move {
            loop {
                let mut child = kill_child.next().await.expect(NEVER_ENDING_STREAM);
                let result = child.kill().await;
                self.0.send(result).await.unwrap();
            }
        }
        .boxed_local()
    }
}

struct BrowserLaunchDriver(CompleteHandle<Result<(), std::io::Error>>);

impl Driver for BrowserLaunchDriver {
    type Init = ();
    type Input = LocalBoxFuture<'static, Url>;
    type Output = LocalBoxFuture<'static, Result<(), std::io::Error>>;
    fn new(_init: Self::Init) -> (Self, Self::Output) {
        let (future, handle) = future_handles::sync::create();
        (Self(handle), future.map(Result::unwrap).boxed_local())
    }

    fn init(self, url: Self::Input) -> LocalBoxFuture<'static, ()> {
        async move {
            self.0.complete(open::that(url.await.as_str()));
            future::pending::<()>().await;
        }
        .boxed_local()
    }
}
