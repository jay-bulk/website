use async_fn_stream::fn_stream;
use futures::{
    channel::mpsc, future::LocalBoxFuture, stream::LocalBoxStream, FutureExt, SinkExt, StreamExt,
};
use tokio::process::{Child, Command};

use super::Driver;

#[derive(Debug)]
pub struct StaticCommandDriver(Command, mpsc::Sender<Result<Child, std::io::Error>>);

impl Driver for StaticCommandDriver {
    type Init = Command;
    type Input = LocalBoxStream<'static, ()>;
    type Output = LocalBoxStream<'static, Result<Child, std::io::Error>>;
    //type Fut = Result<(), mpsc::SendError>

    fn new(command: Self::Init) -> (Self, Self::Output) {
        let (sender, receiver) = mpsc::channel(1);
        let builder_driver = Self(command, sender);
        let output = receiver.boxed_local();
        (builder_driver, output)
    }

    fn init(mut self, start_builder: Self::Input) -> LocalBoxFuture<'static, ()> {
        let mut s = start_builder.map(|_| Ok(self.0.spawn()));
        self.1.send_all(&mut s).map(Result::unwrap).boxed_local()
        // async move {
        //     loop {
        //         start_builder.next().await.unwrap();
        //         let child = self.0.spawn();
        //         self.1.send(child).await.unwrap();
        //     }
        // }
        // .boxed_local()
    }
}
