pub mod child_process_killer;
pub mod command;
pub mod notify;
pub mod open_that;
pub mod println;

use futures::future::LocalBoxFuture;

/// Provides IO capability for a reactive process
pub trait Driver: Sized {
    type Args;
    type Input;
    type Output;

    fn new(init: Self::Args) -> (Self, Self::Output);
    fn init(self, input: Self::Input) -> LocalBoxFuture<'static, ()>;
}
