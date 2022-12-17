use anyhow::{Context, Result};
use rustfix::diagnostics::Diagnostic;
use serde::Deserialize;
use std::{collections::HashSet, fmt::Display, path::PathBuf, process::Command, rc::Rc};

use crate::Options;

#[derive(Debug, Clone)]
pub struct Build {
    inner: Rc<BuildInner>,
}

#[derive(Debug)]
struct BuildInner {
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
        let mode = if options.rustc {
            BuildMode::Rustc
        } else if let Some(script) = &options.verify_error_path {
            BuildMode::Script(script.clone())
        } else {
            BuildMode::Cargo
        };
        Self {
            inner: Rc::new(BuildInner {
                mode,
                input_path: options.path.clone(),
                no_verify: options.no_verify,
            }),
        }
    }

    pub fn build(&self) -> Result<BuildResult> {
        if self.inner.no_verify {
            return Ok(BuildResult {
                reproduces_issue: false,
                no_verify: true,
            });
        }

        let reproduces_issue = match &self.inner.mode {
            BuildMode::Cargo => {
                let mut cmd = Command::new("cargo");
                cmd.arg("build");

                let output =
                    String::from_utf8(cmd.output().context("spawning rustc process")?.stderr)
                        .unwrap();

                output.contains("internal compiler error")
            }
            BuildMode::Script(script_path) => {
                let mut cmd = Command::new(script_path);

                cmd.output().context("spawning script")?.status.success()
            }
            BuildMode::Rustc => {
                let mut cmd = Command::new("rustc");
                cmd.args(["--edition", "2018"]);
                cmd.arg(&self.inner.input_path);

                cmd.output()
                    .context("spawning rustc process")?
                    .status
                    .code()
                    == Some(101)
            }
        };

        Ok(BuildResult {
            reproduces_issue,
            no_verify: self.inner.no_verify,
        })
    }

    pub fn get_diags(&self) -> Result<(Vec<Diagnostic>, Vec<rustfix::Suggestion>)> {
        let diags = match self.inner.mode {
            BuildMode::Cargo => {
                let mut cmd = Command::new("cargo");
                cmd.args(["build", "--message-format=json"]);

                let cmd_output = cmd.output()?;
                let output = String::from_utf8(cmd_output.stdout.clone())?;

                let messages = serde_json::Deserializer::from_str(&output)
                    .into_iter::<CargoJsonCompileMessage>()
                    .collect::<Result<Vec<_>, _>>()?;

                let diags = messages
                    .into_iter()
                    .filter(|msg| msg.reason == "compiler-message")
                    .flat_map(|msg| msg.message)
                    .collect();

                diags
            }
            BuildMode::Rustc => {
                let mut cmd = std::process::Command::new("rustc");
                cmd.args(["--edition", "2018", "--error-format=json"]);
                cmd.arg(&self.inner.input_path);

                let output = cmd.output()?.stderr;
                let output = String::from_utf8(output)?;

                let diags = serde_json::Deserializer::from_str(&output)
                    .into_iter::<Diagnostic>()
                    .collect::<Result<_, _>>()?;

                diags
            }
            BuildMode::Script(_) => todo!(),
        };

        let mut suggestions = Vec::new();
        for cargo_msg in &diags {
            // One diagnostic line might have multiple suggestions
            suggestions.extend(rustfix::collect_suggestions(
                cargo_msg,
                &HashSet::new(),
                rustfix::Filter::Everything,
            ));
        }

        Ok((diags, suggestions))
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
            (false, true) => f.write_str("yes (no-verify)"),
            (false, false) => f.write_str("no"),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CargoJsonCompileMessage {
    pub reason: String,
    pub message: Option<Diagnostic>,
}
