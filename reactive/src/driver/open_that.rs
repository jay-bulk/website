use std::ffi::OsStr;

use futures::{
    channel::mpsc::{self, Sender},
    stream::LocalBoxStream,
    FutureExt, SinkExt, StreamExt,
};

use super::Driver;

pub struct StaticOpenThatDriver<O: AsRef<OsStr> + 'static> {
    os_str: O,
    sender: Sender<std::io::Result<()>>,
}

impl<O: AsRef<OsStr> + 'static> Driver for StaticOpenThatDriver<O> {
    type Args = O;
    type Input = LocalBoxStream<'static, ()>;
    type Output = LocalBoxStream<'static, std::io::Result<()>>;

    fn new(init: Self::Args) -> (Self, Self::Output) {
        let (sender, receiver) = mpsc::channel(1);
        (
            Self {
                os_str: init,
                sender,
            },
            receiver.boxed_local(),
        )
    }

    fn init(self, input: Self::Input) -> futures::future::LocalBoxFuture<'static, ()> {
        let Self { os_str, mut sender } = self;
        let mut opened = input
            .then(move |_| futures::future::ready(Ok(open::that(os_str.as_ref()))))
            .boxed_local();
        async move {
            sender.send_all(&mut opened).map(Result::unwrap).await;
        }
        .boxed_local()
    }
}
