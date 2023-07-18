use futures::future::LocalBoxFuture;

pub trait Driver: Sized {
    type Input;
    type Output;
    fn new() -> (Self, Self::Output);
    fn init(self, input: Self::Input) -> LocalBoxFuture<'static, ()>;
}
