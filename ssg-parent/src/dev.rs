use std::{path::PathBuf, process::ExitStatus};

use async_fn_stream::try_fn_stream;
use camino::{Utf8Path, Utf8PathBuf};
use futures::{future::BoxFuture, stream::BoxStream, FutureExt, Stream, StreamExt, TryStreamExt};
use notify::{recommended_watcher, Event, EventKind, RecursiveMode, Watcher};
use portpicker::Port;
use reqwest::Url;
use thiserror::Error;
use tokio::sync::mpsc;

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

    let inputs = Inputs {
        server_task,
        port,
        builder_crate_fs_change,
        builder_termination,
        launch_browser,
        output_dir,
        browser_launch: todo!(),
    };

    let outputs = app(inputs);

    let Outputs {
        re_start_builder,
        launch_browser,
        error: app_error,
        stderr,
    } = outputs;

    builder_driver.init(re_start_builder);
    let eprintln_task = stderr.for_each(|message| async move { eprintln!("{message}") });

    let browser_launch = launch_browser.map(|port| {
        let url = Url::parse(&format!("http://{LOCALHOST}:{port}")).unwrap();
        open::that(url.as_str())
    });

    todo!()
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
struct BuilderDriver;

impl BuilderDriver {
    fn new() -> (Self, BoxStream<'static, Result<ExitStatus, std::io::Error>>) {
        todo!()
    }

    fn init(&mut self, re_start_builder: impl Stream<Item = ()>) {
        todo!()
    }
}
