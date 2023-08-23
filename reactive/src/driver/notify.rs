use std::{marker::PhantomData, path::PathBuf};

use futures::{
    channel::mpsc,
    executor::block_on,
    future::{pending, LocalBoxFuture},
    stream::LocalBoxStream,
    FutureExt, SinkExt, StreamExt,
};
use notify::{recommended_watcher, RecursiveMode, Watcher, RecommendedWatcher};

use super::Driver;

pub use notify::{Error, Event, EventKind, Result};

pub struct FsChangeDriver<T> {
    watcher: RecommendedWatcher,
    sender: mpsc::Sender<Result<Event>>,
    path: PathBuf,
    boo: PhantomData<fn(T) -> PathBuf>,
}

impl<T> Driver for FsChangeDriver<T>
where
    PathBuf: From<T>,
{
    type Args = T;
    type ConstructionError = notify::Error;
    type Input = ();
    type Output = LocalBoxStream<'static, Result<Event>>;

    fn new(path: Self::Args) -> Result<(Self, Self::Output)> {
        let (sender, receiver) = mpsc::channel::<Result<Event>>(1);

        let watcher = recommended_watcher(move |result: Result<Event>| {
            panic!("our handler");
            block_on(sender.send(result)).expect("this closure gets sent to a blocking context");
            panic!("end of our handler");
        })?;

        let fs_change_driver = Self {
            watcher,
            sender,
            path: path.into(),
            boo: PhantomData,
        };

        Ok((fs_change_driver, receiver.boxed_local()))
    }

    fn init(mut self, _input: Self::Input) -> LocalBoxFuture<'static, ()> {
        let mut sender = self.sender.clone();

        let mut watcher = match self.watcher {
            Ok(watcher) => watcher,
            Err(error) => {
                block_on(self.sender.send(Err(error))).unwrap();
                return pending().boxed_local();
            }
        };

        if let Err(error) = watcher.watch(&self.path, RecursiveMode::Recursive) {
            block_on(self.sender.send(Err(error))).unwrap();
            return pending().boxed_local();
        };

        pending().boxed_local()
    }
}
