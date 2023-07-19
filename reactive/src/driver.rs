pub mod eprintln;




use futures::future::LocalBoxFuture;

pub trait Driver: Sized {
    type Init;
    type Input;
    type Output;

    fn new(init: Self::Init) -> (Self, Self::Output);
    fn init(self, input: Self::Input) -> LocalBoxFuture<'static, ()>;
}
