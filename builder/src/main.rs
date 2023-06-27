#![warn(clippy::all, clippy::pedantic)]

#[macro_use]
mod html;

mod components;
mod constants;
mod expected_files;
mod file_specs;
mod fonts;
mod google_font;
mod graphic_file_specs;
mod markdown;
mod mob;
mod pages;
mod relative_path;
mod style;
mod syn_helpers;
mod url;

use builder::OUTPUT_DIR;
use ssg_child::generate_static_site;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let file_specs = file_specs::get()?;
    let mut generation_task = generate_static_site(OUTPUT_DIR.clone(), file_specs);

    generation_task.set_file_result_fn(|progress_report| {
        eprintln!("{progress_report:?}");
    });

    generation_task.await?;

    Ok(())
}
