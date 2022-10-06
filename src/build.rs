use anyhow::{Context, Result};
use std::path::PathBuf;

pub struct Build {
    path: PathBuf,
}

impl Build {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn build(&self) -> Result<BuildResult> {
        let mut cmd = std::process::Command::new("cargo");

        cmd.current_dir(&self.path).arg("build");

        let output = cmd.output().context("spawning cargo")?;

        Ok(BuildResult {
            success: output.status.success(),
        })
    }
}

pub struct BuildResult {
    pub success: bool,
}
