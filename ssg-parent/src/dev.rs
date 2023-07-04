use std::{path::PathBuf, process::ExitStatus};

use async_fn_stream::{fn_stream, try_fn_stream};
use camino::{Utf8Path, Utf8PathBuf};
use future_handles::sync::CompleteHandle;
use futures::{
    future::{BoxFuture, Remote, RemoteHandle},
    select,
    stream::BoxStream,
    Future, FutureExt, Stream, StreamExt, TryStreamExt,
};
use notify::{recommended_watcher, Event, EventKind, RecursiveMode, Watcher};
use portpicker::Port;

use thiserror::Error;
use tokio::{process::Child, sync::mpsc};
use url::Url;

use crate::rebuild_and_run::WatchError;

#[derive(Debug, Error)]
#[allow(clippy::module_name_repetitions)]
pub enum DevError {
    #[error(transparent)]
    Watch(WatchError),
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

    let (mut builder_driver, builder_termination) = BuilderDriver::new();
    let (mut browser_launch_driver, browser_launch) = BrowserLaunchDriver::new();

    let inputs = Inputs {
        server_task,
        port,
        builder_crate_fs_change,
        builder_termination,
        launch_browser,
        browser_launch,
        output_dir,
    };

    let outputs = app(inputs);

    let Outputs {
        re_start_builder,
        launch_browser,
        error: app_error,
        stderr,
    } = outputs;

    let builder_driver_task = builder_driver.init(re_start_builder);
    let browser_launcher_task = browser_launch_driver.init(launch_browser);
    let eprintln_task = stderr.for_each(|message| async move { eprintln!("{message}") });

    let app_error = select! {
        error = app_error.fuse() => error,
        _ = browser_launcher_task.fuse() => unreachable!(),
        _ = builder_driver_task.fuse() => unreachable!(),
        _ = eprintln_task.fuse() => unreachable!(),
    };

    app_error
}

struct Inputs {
    server_task: BoxFuture<'static, std::io::Error>,
    port: Port,
    builder_crate_fs_change: BoxStream<'static, Result<(), notify::Error>>,
    builder_termination: BoxStream<'static, Result<ExitStatus, std::io::Error>>,
    launch_browser: bool,
    browser_launch: BoxFuture<'static, Result<(), std::io::Error>>,
    output_dir: Utf8PathBuf,
}

struct Outputs {
    re_start_builder: BoxStream<'static, ()>,
    launch_browser: BoxFuture<'static, Port>,
    stderr: BoxStream<'static, String>,
    error: BoxFuture<'static, DevError>,
}

fn app(inputs: Inputs) -> Outputs {
    todo!()
}

#[derive(Debug)]
struct BuilderDriver {
    sender: mpsc::Sender<Result<ExitStatus, std::io::Error>>,
    builder: Option<Child>,
}

impl BuilderDriver {
    fn new() -> (Self, BoxStream<'static, Result<ExitStatus, std::io::Error>>) {
        let (sender, mut receiver) = mpsc::channel(1);
        let stream = try_fn_stream(|emitter| async move {
            loop {
                let input = receiver.recv().await.unwrap()?;
                emitter.emit(input).await;
            }
        })
        .boxed();

        let builder_driver = Self {
            sender,
            builder: None,
        };

        (builder_driver, stream)
    }

    async fn init(self, re_start_builder: impl Stream<Item = ()>) {
        re_start_builder
            .for_each(|_| async move {
                if self.builder.is_none() {
                    self.builder = Self::cargo_run_builder()
                }
                //
                //self.builder.
            })
            .await;
    }

    fn cargo_run_builder() -> Result<Child, std::io::Error> {
        Command::new("cargo")
            .args(["run", "--package", BUILDER_CRATE_NAME])
            .spawn()
    }
}

struct BrowserLaunchDriver(CompleteHandle<Result<(), std::io::Error>>);

impl BrowserLaunchDriver {
    fn new() -> (Self, BoxFuture<'static, Result<(), std::io::Error>>) {
        let (future, handle) = future_handles::sync::create();
        (Self(handle), future.map(|result| result.unwrap()).boxed())
    }

    async fn init(self, launch_browser: impl Future<Output = Port>) {
        let port = launch_browser.await;
        let url = Url::parse(&format!("http://{LOCALHOST}:{port}")).unwrap();
        self.0.complete(open::that(url.as_str()));
    }
}
