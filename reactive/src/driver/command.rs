use futures::{
    channel::mpsc, future::LocalBoxFuture, stream::LocalBoxStream, FutureExt, SinkExt, StreamExt,
};
use tokio::process::{Child, Command};

use super::Driver;

/// The driver is initialised with a command, the command is run when it recieves an input, the output spawns the command as a process or sends back an error.
#[derive(Debug)]
pub struct StaticCommandDriver(Command, mpsc::Sender<Result<Child, std::io::Error>>);

impl Driver for StaticCommandDriver {
    type Args = Command;
    type Input = LocalBoxStream<'static, ()>;
    type Output = LocalBoxStream<'static, Result<Child, std::io::Error>>;

    fn new(command: Self::Args) -> (Self, Self::Output) {
        let (sender, receiver) = mpsc::channel(1);
        let builder_driver = Self(command, sender);
        let output = receiver.boxed_local();
        (builder_driver, output)
    }

    fn init(self, start_builder: Self::Input) -> LocalBoxFuture<'static, ()> {
        let Self(mut command, mut sender) = self;
        let mut s = start_builder.map(move |_| Ok(command.spawn()));
        async move { sender.send_all(&mut s).map(Result::unwrap).await }.boxed_local()
    }
}
