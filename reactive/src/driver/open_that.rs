use std::ffi::OsStr;

use futures::{
    channel::mpsc::{self, Sender},
    stream::LocalBoxStream,
    FutureExt, SinkExt, StreamExt,
};

use super::Driver;

pub struct StaticOpenThatDriver<O : AsRef<OsStr>> { os_str: O, sender: Sender<std::io::Result<()>> }

impl<O : AsRef<OsStr>> Driver for StaticOpenThatDriver<O> {
    type Init = O;
    type Input = LocalBoxStream<'static, Box<dyn AsRef<OsStr>>>;
    type Output = LocalBoxStream<'static, std::io::Result<()>>;

    fn new(init: Self::Init) -> (Self, Self::Output) {
        let (sender, receiver) = mpsc::channel(1);
        (Self{os_str: O,sender}, receiver.boxed_local())
    }

    fn init(self, input: Self::Input) -> futures::future::LocalBoxFuture<'static, ()> {
        let Self(mut sender) = self;
        let mut opened = input
            .then(|that| futures::future::ready(Ok(open::that(that.as_ref()))))
            .boxed_local();
        async move {
            sender.send_all(&mut opened).map(Result::unwrap).await;
        }
        .boxed_local()
    }
}
