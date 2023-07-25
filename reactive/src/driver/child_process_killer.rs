use futures::{channel::mpsc, future::LocalBoxFuture, stream::LocalBoxStream, SinkExt, StreamExt};
use tokio::process::Child;

use super::Driver;

pub struct ChildProcessKillerDriver(mpsc::Sender<Result<(), std::io::Error>>);

impl Driver for ChildProcessKillerDriver {
    type Init = ();
    type Input = LocalBoxStream<'static, Child>;
    type Output = LocalBoxStream<'static, Result<(), std::io::Error>>;

    fn new(_init: Self::Init) -> (Self, Self::Output) {
        let (sender, mut receiver) = mpsc::channel(1);
        let child_killer_driver = Self(sender);
        (child_killer_driver, receiver.boxed_local())
    }

    fn init(self, mut kill_child: Self::Input) -> LocalBoxFuture<'static, ()> {
        let Self(sender) = self;
        let kill_child = kill_child.map(|child| Ok(child.kill())).boxed_local();
        sender.send_all(&mut kill_child)
    }
}
