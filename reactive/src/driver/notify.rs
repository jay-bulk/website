use std::{marker::PhantomData, path::PathBuf};

use futures::{
    channel::mpsc,
    executor::block_on,
    future::{pending, LocalBoxFuture},
    stream::LocalBoxStream,
    FutureExt, SinkExt, StreamExt,
};
use notify::{recommended_watcher, RecursiveMode, Watcher};

use super::Driver;

pub use notify::{Error, Event, EventKind, Result};

pub struct FsChangeDriver<T> {
    sender: mpsc::Sender<Result<Event>>,
    path: PathBuf,
    boo: PhantomData<fn(T) -> PathBuf>,
}

impl<T> Driver for FsChangeDriver<T>
where
    PathBuf: From<T>,
{
    type Args = T;
    type Input = ();
    type Output = LocalBoxStream<'static, Result<Event>>;

    fn new(path: Self::Args) -> (Self, Self::Output) {
        let (sender, receiver) = mpsc::channel::<Result<Event>>(1);

        let path = path.into();

        let pwd = std::env::current_dir().unwrap();

        let path 

        let fs_change_driver = Self {
            sender,
            path,
            boo: PhantomData,
        };

        (fs_change_driver, receiver.boxed_local())
    }

    fn init(mut self, _input: Self::Input) -> LocalBoxFuture<'static, ()> {
        let mut sender = self.sender.clone();

        let watcher = recommended_watcher(move |result: Result<Event>| {
            println!("watcher is called");
            block_on(sender.send(result)).expect("this closure gets sent to a blocking context");
        });

        let mut watcher = match watcher {
            Ok(watcher) => watcher,
            Err(error) => {
                block_on(self.sender.send(Err(error))).unwrap();
                return pending().boxed_local();
            }
        };

        if let Err(error) = watcher.watch(&self.path, RecursiveMode::Recursive) {
            dbg!("{error}");
            block_on(self.sender.send(Err(error))).unwrap();
            return pending().boxed_local();
        };

        pending().boxed_local()
    }
}
