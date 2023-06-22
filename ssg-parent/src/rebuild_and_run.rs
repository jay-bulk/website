use thiserror::Error;
use tokio::process::Command;
use watchexec::{config::{InitConfig, RuntimeConfig}, ErrorHook};

#[derive(Error, Debug)]
pub enum WatchError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Exit(std::process::ExitStatus),
}

pub async fn watch_for_changes_and_rebuild() -> WatchError {
    let mut init_config = InitConfig::default();

    init_config.on_error(|error: ErrorHook| async {
        eprintln!("{}", error.error);
        Ok(())
    });

    let runtime_config = RuntimeConfig::default();

    runtime_config.pathset();

    WatchError::Exit(status)
}
