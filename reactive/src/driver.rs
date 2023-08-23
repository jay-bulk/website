pub mod child_process_killer;
pub mod command;
pub mod notify;
pub mod open_that;
pub mod println;

use futures::future::LocalBoxFuture;

/// Provides IO
pub trait Driver: Sized {
    /// Argument(s) for driver creation.
    type Args;

    /// The input _of the driver_.
    type Input;

    /// The output _of the driver_.
    type Output;

    /// This is how a driver is created.
    fn new(init: Self::Args) -> Result<(Self, Self::Output), Self::ConstructionError>;

    /// This closes the (possible) loop and provides a task that would execute the driver.
    fn init(self, input: Self::Input) -> LocalBoxFuture<'static, ()>;
}
