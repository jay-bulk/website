use std::ffi::OsStr;

use futures::{
    channel::mpsc::{self, Sender},
    stream::LocalBoxStream,
    SinkExt, StreamExt,
};

use super::Driver;

struct OpenThatDriver(Sender<std::io::Result<()>>);

impl Driver for OpenThatDriver {
    type Init = ();
    type Input = LocalBoxStream<'static, Box<dyn AsRef<OsStr>>>;
    type Output = LocalBoxStream<'static, std::io::Result<()>>;

    fn new(init: Self::Init) -> (Self, Self::Output) {
        let (sender, receiver) = mpsc::channel(1);
        (Self(sender), receiver.boxed_local())
    }

    fn init(self, input: Self::Input) -> futures::future::LocalBoxFuture<'static, ()> {
        let Self(mut sender) = self;
        let mut opened = input
            .then(|that| futures::future::ready(open::that(that.as_ref())))
            .boxed_local();
        async move {
            sender.send_all(&mut opened).map(Result::unwrap).await;
        }
        .boxed_local()
    }
}

// struct OpenUrlDriver(CompleteHandle<Result<(), std::io::Error>>);

// impl Driver for OpenUrlDriver {
//     type Init = ();
//     type Input = LocalBoxFuture<'static, Url>;
//     type Output = LocalBoxFuture<'static, Result<(), std::io::Error>>;
//     fn new(_init: Self::Init) -> (Self, Self::Output) {
//         let (future, handle) = future_handles::sync::create();
//         (Self(handle), future.map(Result::unwrap).boxed_local())
//     }

//     fn init(self, url: Self::Input) -> LocalBoxFuture<'static, ()> {
//         async move {
//             self.0.complete(open::that(url.await.as_str()));
//             future::pending::<()>().await;
//         }
//         .boxed_local()
//     }
// }
