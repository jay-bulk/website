use camino::{Utf8Path, Utf8PathBuf};
use futures::{stream, Stream};
use thiserror::Error;

use crate::{
    rebuild_and_run::{watch_for_changes_and_rebuild, WatchError},
    server::start_development_web_server,
};

#[derive(Debug, Error)]
#[allow(clippy::module_name_repetitions)]
pub enum DevError {
    #[error(transparent)]
    Watch(WatchError),
    #[error(transparent)]
    Io(std::io::Error),
}

pub async fn dev<O: AsRef<Utf8Path>>(launch_browser: bool, output_dir: O) -> DevError {

    let outputs = app(inputs);

}

struct Inputs {
builder_termination:Stream<Item = 
}

struct Outputs {
}

fn app(inputs: Inputs) -> Outputs {
    todo!()
}

