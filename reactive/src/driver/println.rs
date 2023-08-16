use futures::{future::{LocalBoxFuture, self}, stream::LocalBoxStream, FutureExt, StreamExt};

use super::Driver;

pub struct EprintlnDriver;

impl Driver for EprintlnDriver {
    type Args = ();
    type Input = LocalBoxStream<'static, String>;
    type Output = ();

    fn new(_init: Self::Args) -> (Self, Self::Output) {
        (Self, ())
    }

    fn init(self, input: Self::Input) -> LocalBoxFuture<'static, ()> {
        input
            .for_each(|string| {
                eprintln!("{string}");
                future::ready(())
            })
            .boxed_local()
    }
}
