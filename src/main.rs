#![warn(clippy::all, clippy::pedantic)]

use anyhow::bail;
use builder::OUTPUT_DIR;
use clap::{Parser, Subcommand};
use ssg_parent::{dev, DevError};

#[derive(Debug, Parser)]
struct Cli {
    #[command(subcommand)]
    mode: Option<Mode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Subcommand)]
enum Mode {
    /// watch for changes and rebuild the website
    /// and start a development web server
    Dev {
        /// open website in a browser
        #[arg(short, long)]
        open: bool,
    },
    /// print the output directory path
    PrintOutputDir,
}

impl Default for Mode {
    fn default() -> Self {
        Mode::Dev { open: true }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<DevError> {
    #[cfg(feature = "tokio_console")]
    console_subscriber::init();

    let cli = Cli::parse();

    match cli.mode.unwrap_or_default() {
        Mode::Dev { open } => return Err(dev(open, OUTPUT_DIR.as_path()).await),
        Mode::PrintOutputDir => print!("{}", OUTPUT_DIR.as_os_str().to_str().unwrap()),
    }

    Ok(())
}
