use std::process::ExitStatus;

use camino::{Utf8Path, Utf8PathBuf};
use futures::{
    future::BoxFuture,
    stream::{self, BoxStream},
    Stream,
};
use portpicker::Port;
use thiserror::Error;

use crate::{
    rebuild_and_run::{watch_for_changes_and_rebuild, WatchError},
    server::start_development_web_server,
};

#[derive(Debug, Error)]
#[allow(clippy::module_name_repetitions)]
pub enum DevError {
    #[error(transparent)]
    Watch(WatchError),
    #[error(transparent)]
    Io(std::io::Error),
}

pub async fn dev<O: AsRef<Utf8Path>>(launch_browser: bool, output_dir: O) -> DevError {
    let output_dir = output_dir.as_ref().to_owned();
    

    let inputs = Inputs {
        server_error,
        port,
        builder_crate_fs_change,
        builder_termination,
        launch_browser,
        output_dir,
    };

    let outputs = app(inputs);

    let Outputs {
        builder_invocation,
        launch_browser,
        error,
    } = outputs;

    error.await
}

struct Inputs {
    server_error: BoxFuture<'static, std::io::Error>,
    port: Port,
    builder_crate_fs_change: BoxStream<'static, ()>,
    builder_termination: BoxStream<'static, Result<ExitStatus, std::io::Error>>,
    launch_browser: bool,
    output_dir: Utf8PathBuf,
}

struct Outputs {
    builder_invocation: BoxStream<'static, ()>,
    launch_browser: BoxFuture<'static, Port>,
    stderr: BoxStream<'static, String>,
    error: BoxFuture<'static, DevError>,
}

fn app(inputs: Inputs) -> Outputs {
    todo!()
}
