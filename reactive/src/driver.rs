mod child_process_killer;
mod eprintln;
mod static_command;

pub use child_process_killer::ChildProcessKillerDriver;
pub use eprintln::EprintlnDriver;
pub use static_command::StaticCommandDriver;

use futures::future::LocalBoxFuture;

pub trait Driver: Sized {
    type Init;
    type Input;
    type Output;

    fn new(init: Self::Init) -> (Self, Self::Output);
    fn init(self, input: Self::Input) -> LocalBoxFuture<'static, ()>;
}
