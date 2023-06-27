use std::io::{stdout, Write};

use anyhow::ensure;
use camino::Utf8PathBuf;
use tokio::process::Command;

use crate::OUTPUT_DIR;

pub(crate) async fn execute() -> anyhow::Result<()> {
    let output = Command::new("npx")
        .args([
            "tailwindcss",
            "--input",
            [env!("CARGO_MANIFEST_DIR"), "src", "input.css"]
                .iter()
                .collect::<Utf8PathBuf>()
                .as_ref(),
            "--output",
            [".".as_ref(), OUTPUT_DIR.as_path(), "index.css".as_ref()]
                .iter()
                .collect::<Utf8PathBuf>()
                .as_ref(),
            "--content",
            // TODO explicit list instead of pattern
            [
                ".".as_ref(),
                OUTPUT_DIR.as_path(),
                "**".as_ref(),
                "*.html".as_ref(),
            ]
            .iter()
            .collect::<Utf8PathBuf>()
            .as_ref(),
        ])
        .output()
        .await?;

    stdout().write_all(&output.stderr)?;

    ensure!(output.status.success());
    Ok(())
}
