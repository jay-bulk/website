use std::convert::Infallible;

use thiserror::Error;
use tokio::task::JoinError;
use watchexec::{
    action::{Action, Outcome},
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
    #[error("unexpected termination")]
    UnexpectedTermination,
    #[error(transparent)]
    Join(#[from] JoinError),
}

pub async fn watch_for_changes_and_rebuild() -> WatchError {
    let mut init_config = InitConfig::default();

    init_config.on_error(|error: ErrorHook| async move {
        eprintln!("{}", error.error);
        Result::<(), Infallible>::Ok(())
    });

    let mut runtime_config = RuntimeConfig::default();

    runtime_config.pathset(["builder"]);

    runtime_config.on_action(move |action: Action| {
        use Outcome::*;
        action.outcome(Outcome::if_running(Outcome::both(Stop, Start), Start));

        async { Result::<(), Infallible>::Ok(()) }
    });

    let watchexec = match Watchexec::new(init_config, runtime_config.clone()) {
        Ok(watchexec) => watchexec,
        Err(error) => return error.into(),
    };

    match watchexec.main().await {
        Ok(_) => WatchError::UnexpectedTermination,
        Err(error) => error.into(),
    }
}
