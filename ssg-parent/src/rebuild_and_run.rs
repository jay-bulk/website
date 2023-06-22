use std::process::ExitStatus;

use thiserror::Error;
use tokio::process::Command;
use watchexec::{
    config::{InitConfig, RuntimeConfig},
    error::CriticalError,
    ErrorHook, Watchexec,
};

#[derive(Error, Debug)]
pub enum WatchError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Exit(std::process::ExitStatus),
    #[error(transparent)]
    Critical(#[from] CriticalError),
}

pub async fn watch_for_changes_and_rebuild() -> WatchError {
    let mut init_config = InitConfig::default();

    init_config.on_error(|error: ErrorHook| async {
        eprintln!("{}", error.error);
        Ok(())
    });

    let runtime_config = RuntimeConfig::default();

    runtime_config.pathset(["builder"]);

    let watchexec = match Watchexec::new(init_config, runtime_config) {
        Ok(watchexec) => watchexec,
        Err(error) => return error.into(),
    };

    runtime_config.command(command);

    WatchError::Exit(ExitStatus::)
}
