use std::convert::Infallible;

use async_fn_stream::fn_stream;
use futures_util::{FutureExt, select};
use thiserror::Error;
use tokio::task::JoinError;
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

    let watchexec = match Watchexec::new(init_config, runtime_config.clone()) {
        Ok(watchexec) => watchexec,
        Err(error) => return error.into(),
    };

    let stream = fn_stream(|emitter| async {
        runtime_config
            .on_action(move |_action| emitter.emit(()).map(|s| Result::<(), Infallible>::Ok(())));
    });



    let main = watchexec.main();

    select! {

    }

    match main.await {
        Ok(_) => WatchError::UnexpectedTermination,
        Err(error) => error.into(),
    };

}
