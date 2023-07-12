#![warn(clippy::all, clippy::pedantic)]

mod dev;
mod rebuild_and_run;
mod server;

pub use dev::{dev, DevError};
