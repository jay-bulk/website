use std::{path::PathBuf, rc::Rc};

use async_fn_stream::{fn_stream, try_fn_stream};
use camino::Utf8Path;
use future_handles::sync::CompleteHandle;
use futures::{
    future::{self, LocalBoxFuture},
    select,
    stream::{self, LocalBoxStream},
    Future, FutureExt, StreamExt, TryStreamExt,
};
use notify::{recommended_watcher, Event, EventKind, RecursiveMode, Watcher};
use portpicker::Port;
use shared_stream::Share;
use thiserror::Error;
use tokio::{
    process::{Child, Command},
    sync::mpsc,
};
use url::Url;

const NEVER_ENDING_STREAM: &str = "never ending stream";

#[derive(Debug, Error, Clone)]
#[allow(clippy::module_name_repetitions)]
pub enum DevError {
    #[error(transparent)]
    FsWatch(Rc<notify::Error>),
    #[error(transparent)]
    Io(Rc<std::io::Error>),
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

    let (builder_driver, builder_started) = BuilderDriver::new();
    let (child_killer_driver, child_killed) = ChildKillerDriver::new();
    let (browser_launch_driver, browser_launch) = BrowserLaunchDriver::new();

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
        launch_browser,
        error: app_error,
        stderr,
        kill_child,
        run_builder,
    } = outputs;

    let builder_driver_task = builder_driver.init(run_builder);
    let child_killer_task = child_killer_driver.init(kill_child);
    let browser_launcher_task = browser_launch_driver.init(launch_browser);
    let eprintln_task = stderr.for_each(|message| async move { eprintln!("{message}") });

    let app_error = select! {
        error = app_error.fuse() => error,
        _ = builder_driver_task.fuse() => unreachable!(),
        _ = child_killer_task.fuse() => unreachable!(),
        _ = browser_launcher_task.fuse() => unreachable!(),
        _ = eprintln_task.fuse() => unreachable!(),
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

#[derive(Clone)]
enum OutputEvent {
    RunBuilder,
    KillChild(Rc<Child>),
    Stderr(String),
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
    kill_child: LocalBoxStream<'static, Rc<Child>>,
    run_builder: LocalBoxStream<'static, ()>,
    launch_browser: LocalBoxFuture<'static, Port>,
    stderr: LocalBoxStream<'static, String>,
    error: LocalBoxFuture<'static, DevError>,
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

    let initial = stream::once(future::ready(OutputEvent::RunBuilder));

    let reaction = stream::select_all([
        stream::once(server_task).boxed_local(),
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
                Err(error) => Some(OutputEvent::Error(DevError::Io(Rc::new(error)))),
            },
            InputEvent::BuilderCrateFsChange(result) => match result {
                Ok(_) => match &mut state.builder {
                    BuilderState::Starting => {
                        state.builder = BuilderState::Obsolete;
                        None
                    }
                    BuilderState::Started(_) => {
                        let child = state.builder.killing().unwrap();
                        Some(OutputEvent::KillChild(Rc::new(child)))
                    }
                    BuilderState::None | BuilderState::Obsolete => None,
                },
                Err(error) => Some(OutputEvent::Error(DevError::FsWatch(Rc::new(error)))),
            },
            InputEvent::BuilderStarted(child) => match child {
                Ok(child) => match state.builder {
                    BuilderState::None | BuilderState::Starting => {
                        state.builder = BuilderState::Started(child);
                        None
                    }
                    BuilderState::Obsolete => {
                        state.builder = BuilderState::None;
                        Some(OutputEvent::KillChild(Rc::new(child)))
                    }
                    BuilderState::Started(_) => {
                        // TODO is this reachable?
                        let current_child = state.builder.killing().unwrap();
                        state.builder = BuilderState::Started(child);
                        Some(OutputEvent::KillChild(Rc::new(current_child)))
                    }
                },
                Err(error) => Some(OutputEvent::Error(DevError::Io(Rc::new(error)))),
            },
            InputEvent::BrowserLaunched(result) => match result {
                Ok(_) => None,
                Err(error) => Some(OutputEvent::Error(DevError::Io(Rc::new(error)))),
            },
        };

        future::ready(Some(emit))
    })
    .filter_map(future::ready);

    let output = initial.chain(reaction).shared();

    let kill_child = output
        .clone()
        .filter_map(|output| {
            let child = if let OutputEvent::KillChild(child) = output {
                Some(child)
            } else {
                None
            };

            future::ready(child)
        })
        .boxed_local();

    let start_builder = output
        .clone()
        .filter_map(|output| {
            let output = if let OutputEvent::RunBuilder = output {
                Some(())
            } else {
                None
            };

            future::ready(output)
        })
        .boxed_local();

    let stderr = output
        .clone()
        .filter_map(|output| {
            let output = if let OutputEvent::Stderr(message) = output {
                Some(message)
            } else {
                None
            };

            future::ready(output)
        })
        .boxed_local();

    let error = output
        .filter_map(|output| {
            let output = if let OutputEvent::Error(error) = output {
                Some(error)
            } else {
                None
            };

            future::ready(output)
        })
        .into_future()
        .map(|(error, _tail_of_stream)| error.expect(NEVER_ENDING_STREAM))
        .boxed_local();

    let launch_browser = if launch_browser {
        future::ready(port).boxed_local()
    } else {
        future::pending().boxed_local()
    };

    Outputs {
        kill_child,
        run_builder: start_builder,
        stderr,
        error,
        launch_browser,
    }
}

#[derive(Debug)]
struct BuilderDriver(mpsc::Sender<Result<Child, std::io::Error>>);

impl BuilderDriver {
    fn new() -> (Self, LocalBoxStream<'static, Result<Child, std::io::Error>>) {
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

    async fn init(self, mut start_builder: LocalBoxStream<'static, ()>) {
        loop {
            start_builder.next().await.unwrap();
            let child = Self::cargo_run_builder();
            self.0.send(child).await.unwrap();
        }
    }

    fn cargo_run_builder() -> Result<Child, std::io::Error> {
        Command::new("cargo")
            .args(["run", "--package", BUILDER_CRATE_NAME])
            .spawn()
    }
}

struct ChildKillerDriver(mpsc::Sender<Result<(), std::io::Error>>);

impl ChildKillerDriver {
    fn new() -> (Self, LocalBoxStream<'static, Result<(), std::io::Error>>) {
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

    async fn init(self, mut kill_child: LocalBoxStream<'static, Rc<Child>>) {
        loop {
            let child = kill_child.next().await.expect(NEVER_ENDING_STREAM);
            let mut child = rc_try_unwrap_recursive(Err(child)).await;
            let result = child.kill().await;
            self.0.send(result).await.unwrap();
        }
    }
}

// TODO trait method
fn rc_try_unwrap_recursive<T: 'static>(result: Result<T, Rc<T>>) -> LocalBoxFuture<'static, T> {
    async {
        match result {
            Ok(inner) => inner,
            Err(rc) => rc_try_unwrap_recursive(Rc::try_unwrap(rc)).await,
        }
    }
    .boxed_local()
}

struct BrowserLaunchDriver(CompleteHandle<Result<(), std::io::Error>>);

impl BrowserLaunchDriver {
    fn new() -> (Self, LocalBoxFuture<'static, Result<(), std::io::Error>>) {
        let (future, handle) = future_handles::sync::create();
        (Self(handle), future.map(Result::unwrap).boxed_local())
    }

    async fn init(self, launch_browser: impl Future<Output = Port>) {
        let port = launch_browser.await;
        let url = Url::parse(&format!("http://{LOCALHOST}:{port}")).unwrap();
        self.0.complete(open::that(url.as_str()));
    }
}
