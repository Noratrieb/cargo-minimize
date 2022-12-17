use anyhow::{Context, Result};
use std::{fmt::Display, path::PathBuf};

use crate::Options;

#[derive(Debug)]
pub struct Build {
    mode: BuildMode,
    input_path: PathBuf,
}

#[derive(Debug)]
enum BuildMode {
    Cargo,
    Script(PathBuf),
    Rustc,
}

impl Build {
    pub fn new(options: &Options) -> Self {
        let mode = if options.cargo {
            BuildMode::Cargo
        } else if let Some(script) = &options.verify_error_path {
            BuildMode::Script(script.clone())
        } else {
            BuildMode::Rustc
        };
        Self {
            mode,
            input_path: options.path.clone(),
        }
    }

    pub fn build(&self) -> Result<BuildResult> {
        let reproduces_issue = match &self.mode {
            BuildMode::Cargo => {
                let mut cmd = std::process::Command::new("cargo");
                cmd.arg("build");

                let output =
                    String::from_utf8(cmd.output().context("spawning rustc process")?.stderr)
                        .unwrap();

                output.contains("internal compiler error")
            }
            BuildMode::Script(script_path) => {
                let mut cmd = std::process::Command::new(script_path);

                cmd.output().context("spawning script")?.status.success()
            }
            BuildMode::Rustc => {
                let mut cmd = std::process::Command::new("rustc");
                cmd.args(["--edition", "2018"]);
                cmd.arg(&self.input_path);

                cmd.output()
                    .context("spawning rustc process")?
                    .status
                    .code()
                    == Some(101)
            }
        };

        Ok(BuildResult { reproduces_issue })
    }
}

pub struct BuildResult {
    pub reproduces_issue: bool,
}

impl Display for BuildResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.reproduces_issue {
            true => f.write_str("yes"),
            false => f.write_str("no"),
        }
    }
}
