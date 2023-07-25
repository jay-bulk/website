use async_fn_stream::fn_stream;
use futures::{channel::mpsc, stream::LocalBoxStream};
use tokio::process::{Child, Command};

use super::Driver;

#[derive(Debug)]
pub struct StaticCommandDriver(Command, mpsc::Sender<Result<Child, std::io::Error>>);

impl Driver for StaticCommandDriver {
    type Init = Command;
    type Input = LocalBoxStream<'static, ()>;
    type Output = LocalBoxStream<'static, Result<Child, std::io::Error>>;

    fn new(command: Self::Init) -> (Self, Self::Output) {
        let (sender, mut receiver) = tokio::mpsc::channel(1);
        let stream = fn_stream(|emitter| async move {
            loop {
                let input = receiver.recv().unwrap();
                emitter.emit(input).await;
            }
        })
        .boxed();

        let builder_driver = Self(command, sender);

        (builder_driver, stream)
    }

    fn init(mut self, mut start_builder: Self::Input) -> LocalBoxFuture<'static, ()> {
        async move {
            loop {
                start_builder.next().await.unwrap();
                let child = self.0.spawn();
                self.1.send(child).await.unwrap();
            }
        }
        .boxed_local()
    }
}
