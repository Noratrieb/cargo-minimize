use anyhow::{Context, Result};
use std::{fmt::Display, path::PathBuf};

#[derive(Debug)]
pub struct Build {
    cargo: bool,
    script_path: Option<PathBuf>,
    input_path: PathBuf,
}

impl Build {
    pub fn new(cargo: bool, script_path: Option<PathBuf>, input_path: PathBuf) -> Self {
        Self {
            cargo,
            script_path,
            input_path,
        }
    }

    pub fn build(&self) -> Result<BuildResult> {
        let reproduces_issue = if self.cargo {
            let mut cmd = std::process::Command::new("cargo");
            cmd.arg("build");

            let output =
                String::from_utf8(cmd.output().context("spawning rustc process")?.stderr).unwrap();

            output.contains("internal compiler error")
        } else if let Some(script_path) = &self.script_path {
            let mut cmd = std::process::Command::new(script_path);

            cmd.output().context("spawning script")?.status.success()
        } else {
            let mut cmd = std::process::Command::new("rustc");
            cmd.args(["--edition", "2018"]);
            cmd.arg(&self.input_path);

            cmd.output()
                .context("spawning rustc process")?
                .status
                .code()
                == Some(101)
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
