use futures::{
    channel::mpsc, future::LocalBoxFuture, stream::LocalBoxStream, FutureExt, SinkExt, StreamExt,
};
use tokio::process::Child;

use super::Driver;

pub struct ChildProcessKillerDriver(mpsc::Sender<Result<(), std::io::Error>>);

impl Driver for ChildProcessKillerDriver {
    type Init = ();
    type Input = LocalBoxStream<'static, Child>;
    type Output = LocalBoxStream<'static, Result<(), std::io::Error>>;

    fn new(_init: Self::Init) -> (Self, Self::Output) {
        let (sender, receiver) = mpsc::channel(1);
        let child_killer_driver = Self(sender);
        (child_killer_driver, receiver.boxed_local())
    }

    fn init(self, kill_child: Self::Input) -> LocalBoxFuture<'static, ()> {
        let Self(mut sender) = self;
        let mut kill_child = kill_child
            .then(|mut child| async move { Ok(child.kill().await) })
            .boxed_local();
        async move {
            sender.send_all(&mut kill_child).map(Result::unwrap).await;
        }
        .boxed_local()
    }
}
