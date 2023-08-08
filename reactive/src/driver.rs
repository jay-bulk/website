pub mod child_process_killer;
pub mod command;
pub mod eprintln;
pub mod open_that;
pub mod notify;

use futures::future::LocalBoxFuture;

pub trait Driver: Sized {
    type Args;
    type Input;
    type Output;

    fn new(init: Self::Args) -> (Self, Self::Output);
    fn init(self, input: Self::Input) -> LocalBoxFuture<'static, ()>;
}
