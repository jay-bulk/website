use std::convert::Infallible;

use futures::{
    future::{self, LocalBoxFuture},
    stream::LocalBoxStream,
    FutureExt, StreamExt,
};

use super::Driver;

pub struct EprintlnDriver;

impl Driver for EprintlnDriver {
    type Args = ();
    type ConstructionError = Infallible;
    type Input = LocalBoxStream<'static, String>;
    type Output = ();

    fn new(_init: Self::Args) -> Result<(Self, Self::Output), Self::ConstructionError> {
        Ok((Self, ()))
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
