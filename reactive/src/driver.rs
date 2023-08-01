mod child_process_killer;
mod command;
mod eprintln;
mod open_that;

pub use child_process_killer::ChildProcessKillerDriver;
pub use command::StaticCommandDriver;
pub use eprintln::EprintlnDriver;
pub use open_that::StaticOpenThatDriver;

use futures::future::LocalBoxFuture;

pub trait Driver: Sized {
    type Init;
    type Input;
    type Output;

    fn new(init: Self::Init) -> (Self, Self::Output);
    fn init(self, input: Self::Input) -> LocalBoxFuture<'static, ()>;
}
