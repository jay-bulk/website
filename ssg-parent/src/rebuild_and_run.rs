use std::path::PathBuf;

use async_fn_stream::try_fn_stream;
use futures::{future::BoxFuture, TryStreamExt};
use notify::{recommended_watcher, Event, EventKind, RecursiveMode, Watcher};
use thiserror::Error;
use tokio::{
    process::{Child, Command},
    sync::mpsc,
};

#[derive(Error, Debug)]
pub enum WatchError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Notify(#[from] notify::Error),
}

const BUILDER_CRATE_NAME: &str = "builder";

pub async fn watch_for_changes_and_rebuild(
    post_build: impl FnMut() -> BoxFuture<'static, Result<(), Box<dyn std::error::Error>>>,
) -> WatchError {
    let cargo_run_builder_process = match cargo_run_builder() {
        Ok(cargo_run_builder_process) => cargo_run_builder_process,
        Err(error) => return error.into(),
    };

    try_fn_stream(|emitter| async move {
        let (sender, mut receiver) = mpsc::channel(1);

        let mut watcher = recommended_watcher(move |result: Result<Event, notify::Error>| {
            sender.blocking_send(result).unwrap();
        })?;

        watcher.watch(&PathBuf::from(BUILDER_CRATE_NAME), RecursiveMode::Recursive)?;

        loop {
            let event = receiver.recv().await.unwrap()?;
            emitter.emit(event).await;
        }
    })
    .try_fold(
        cargo_run_builder_process,
        |mut cargo_run_builder_process, event| async move {
            if let EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) = event.kind {
                cargo_run_builder_process.kill().await?;
                Result::<_, WatchError>::Ok(cargo_run_builder()?)
            } else {
                Ok(cargo_run_builder_process)
            }
        },
    )
    .await
    .expect_err("should end only in the case of error")
}

fn cargo_run_builder() -> Result<Child, std::io::Error> {
    Command::new("cargo")
        .args(["run", "--package", BUILDER_CRATE_NAME])
        .spawn()
}
