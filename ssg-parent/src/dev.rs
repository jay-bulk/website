use std::path::PathBuf;

use async_fn_stream::{fn_stream, try_fn_stream};
use camino::{Utf8Path, Utf8PathBuf};
use future_handles::sync::CompleteHandle;
use futures::{
    future::BoxFuture, select, stream::BoxStream, Future, FutureExt, StreamExt, TryStreamExt,
};
use notify::{recommended_watcher, Event, EventKind, RecursiveMode, Watcher};
use portpicker::Port;

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
    Watch(notify::Error),
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

    let (builder_driver, builder_child) = BuilderDriver::new();
    let (child_killer_driver, child_killed) = ChildKillerDriver::new();
    let (browser_launch_driver, browser_launch) = BrowserLaunchDriver::new();

    let inputs = Inputs {
        server_task,
        port,
        builder_crate_fs_change,
        launch_browser,
        browser_launch,
        output_dir,
        builder_started: builder_child,
        child_killed,
    };

    let outputs = app(inputs);

    let Outputs {
        launch_browser,
        error: app_error,
        stderr,
        kill_child,
        start_builder,
    } = outputs;

    let builder_driver_task = builder_driver.init(start_builder);
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

struct Inputs {
    server_task: BoxFuture<'static, std::io::Error>,
    port: Port,
    child_killed: BoxStream<'static, Result<(), std::io::Error>>,
    builder_crate_fs_change: BoxStream<'static, Result<(), notify::Error>>,
    builder_started: BoxStream<'static, Result<Child, std::io::Error>>,
    launch_browser: bool,
    browser_launch: BoxFuture<'static, Result<(), std::io::Error>>,
    output_dir: Utf8PathBuf,
}

enum StreamInput {
    ChildKilled,
    BuilderCrateFsChange,
    BuilderStarted(Child),
}

struct Outputs {
    kill_child: BoxStream<'static, Child>,
    start_builder: BoxStream<'static, ()>,
    launch_browser: BoxFuture<'static, Port>,
    stderr: BoxStream<'static, String>,
    error: BoxFuture<'static, DevError>,
}

#[derive(Debug, Default)]
struct State {
    builder_child: Option<Child>,
}

fn app(inputs: Inputs) -> Outputs {
    let Inputs {
        server_task,
        port,
        child_killed,
        builder_crate_fs_change,
        builder_started,
        launch_browser,
        browser_launch,
        output_dir,
    } = inputs;

    let child_killed = child_killed
        .map(|result| match result {
            Ok(_) => Ok(StreamInput::ChildKilled),
            Err(error) => Err(DevError::Io(error)),
        })
        .boxed_local();

    let builder_crate_fs_change = builder_crate_fs_change
        .map(|result| match result {
            Ok(_) => Ok(StreamInput::BuilderCrateFsChange),
            Err(error) => Err(DevError::Watch(error)),
        })
        .boxed_local();

    let builder_started = builder_started
        .map(|result| match result {
            Ok(child) => Ok(StreamInput::BuilderStarted(child)),
            Err(error) => Err(DevError::Io(error)),
        })
        .boxed_local();

    let temp =
        futures::stream::select_all([child_killed, builder_crate_fs_change, builder_started]).scan(
            State::default(),
            |mut state, input| async move {
                //
                Some(true)
            },
        );

    todo!()
}

#[derive(Debug)]
struct BuilderDriver(mpsc::Sender<Result<Child, std::io::Error>>);

impl BuilderDriver {
    fn new() -> (Self, BoxStream<'static, Result<Child, std::io::Error>>) {
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

    async fn init(self, mut start_builder: BoxStream<'static, ()>) {
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
    fn new() -> (Self, BoxStream<'static, Result<(), std::io::Error>>) {
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

    async fn init(self, mut kill_child: BoxStream<'static, Child>) {
        loop {
            let mut child = kill_child.next().await.expect(NEVER_ENDING_STREAM);
            let result = child.kill().await;
            self.0.send(result).await.unwrap();
        }
    }
}

struct BrowserLaunchDriver(CompleteHandle<Result<(), std::io::Error>>);

impl BrowserLaunchDriver {
    fn new() -> (Self, BoxFuture<'static, Result<(), std::io::Error>>) {
        let (future, handle) = future_handles::sync::create();
        (Self(handle), future.map(Result::unwrap).boxed())
    }

    async fn init(self, launch_browser: impl Future<Output = Port>) {
        let port = launch_browser.await;
        let url = Url::parse(&format!("http://{LOCALHOST}:{port}")).unwrap();
        self.0.complete(open::that(url.as_str()));
    }
}
