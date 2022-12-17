use anyhow::{Context, Result};
use rustfix::diagnostics::Diagnostic;
use std::{collections::HashSet, fmt::Display, path::PathBuf};

use crate::Options;

#[derive(Debug)]
pub struct Build {
    mode: BuildMode,
    input_path: PathBuf,
    no_verify: bool,
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
            no_verify: options.no_verify,
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

        Ok(BuildResult {
            reproduces_issue,
            no_verify: self.no_verify,
        })
    }

    pub fn get_suggestions(&self) -> Result<(Vec<Diagnostic>, Vec<rustfix::Suggestion>)> {
        match self.mode {
            BuildMode::Cargo => {
                todo!();
            }
            BuildMode::Script(_) => todo!(),
            BuildMode::Rustc => {
                let mut cmd = std::process::Command::new("rustc");
                cmd.args(["--edition", "2018", "--error-format=json"]);
                cmd.arg(&self.input_path);

                let output = cmd.output()?.stderr;
                let output = String::from_utf8(output)?;

                let diags = serde_json::Deserializer::from_str(&output).into_iter::<Diagnostic>().collect::<Result<_, _>>()?;

                let suggestions = rustfix::get_suggestions_from_json(
                    &output,
                    &HashSet::new(),
                    rustfix::Filter::Everything,
                )
                .context("reading output as rustfix suggestions")?;

                Ok((diags, suggestions))
            }
        }
    }
}

pub struct BuildResult {
    reproduces_issue: bool,
    no_verify: bool,
}

impl BuildResult {
    pub fn reproduces_issue(&self) -> bool {
        self.reproduces_issue || self.no_verify
    }
}

impl Display for BuildResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.reproduces_issue, self.no_verify) {
            (true, _) => f.write_str("yes"),
            (false, true) => f.write_str("no (ignore)"),
            (false, false) => f.write_str("no"),
        }
    }
}
