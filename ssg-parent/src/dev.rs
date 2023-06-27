use camino::{Utf8Path, Utf8PathBuf};
use futures::future::BoxFuture;
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

pub async fn dev<O: AsRef<Utf8Path>>(
    launch_browser: bool,
    output_dir: O,
    post_build: fn() -> BoxFuture<'static, Result<(), Box<dyn std::error::Error>>>,
) -> DevError {
    tokio::select! {
        error = watch_for_changes_and_rebuild() => { DevError::Watch(error) },
        error = start_development_web_server(launch_browser, Utf8PathBuf::from(output_dir.as_ref())) => { DevError::Io(error) },
    }
}
