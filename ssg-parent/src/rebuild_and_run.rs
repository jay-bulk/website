use thiserror::Error;
use tokio::process::Command;

#[derive(Error, Debug)]
pub enum WatchError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Exit(std::process::ExitStatus),
}

pub async fn watch_for_changes_and_rebuild() -> WatchError {


    WatchError::Exit(status)
}
