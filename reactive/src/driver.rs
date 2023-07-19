pub mod eprintln {
    use futures::{future::LocalBoxFuture, stream::LocalBoxStream, FutureExt, StreamExt};

    use super::Driver;

    pub struct EprintlnDriver;

    impl Driver for EprintlnDriver {
        type Init = ();
        type Input = LocalBoxStream<'static, String>;
        type Output = ();

        fn new(_init: Self::Init) -> (Self, Self::Output) {
            (Self, ())
        }

        fn init(self, input: Self::Input) -> LocalBoxFuture<'static, ()> {
            input
                .for_each(|string| {
                    eprintln!("{string}");
                    futures::future::ready(())
                })
                .boxed_local()
        }
    }
}

use futures::future::LocalBoxFuture;

pub trait Driver: Sized {
    type Init;
    type Input;
    type Output;

    fn new(init: Self::Init) -> (Self, Self::Output);
    fn init(self, input: Self::Input) -> LocalBoxFuture<'static, ()>;
}
