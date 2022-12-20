use anyhow::{bail, Context, Result};
use rustfix::diagnostics::Diagnostic;
use serde::Deserialize;
use std::{
    collections::HashSet,
    ffi::OsStr,
    fmt::{Debug, Display},
    path::PathBuf,
    process::Command,
    rc::Rc,
};

use crate::{dylib_flag::RustFunction, EnvVar, Options};

#[derive(Debug, Clone)]
pub struct Build {
    inner: Rc<BuildInner>,
}

pub enum Verify {
    Ice,
    Custom(RustFunction),
    None,
}

impl Debug for Verify {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ice => write!(f, "Ice"),
            Self::Custom(_) => f.debug_tuple("Custom").finish(),
            Self::None => write!(f, "None"),
        }
    }
}

#[derive(Debug)]
struct BuildInner {
    mode: BuildMode,
    input_path: PathBuf,
    verify: Verify,
    env: Vec<EnvVar>,
    allow_color: bool,
    project_dir: Option<PathBuf>,
}

#[derive(Debug)]
enum BuildMode {
    Cargo { args: Option<Vec<String>> },
    Script(PathBuf),
    Rustc,
}

impl Build {
    pub fn new(options: &Options) -> Self {
        let mode = if options.rustc {
            BuildMode::Rustc
        } else if let Some(script) = &options.script_path {
            BuildMode::Script(script.clone())
        } else {
            BuildMode::Cargo {
                args: options
                    .cargo_args
                    .as_ref()
                    .map(|cmd| cmd.split_whitespace().map(ToString::to_string).collect()),
            }
        };

        let verify = if options.no_verify {
            Verify::None
        } else if let Some(func) = options.verify_fn {
            Verify::Custom(func)
        } else {
            Verify::Ice
        };

        Self {
            inner: Rc::new(BuildInner {
                mode,
                input_path: options.path.clone(),
                verify,
                env: options.env.clone(),
                allow_color: !options.no_color,
                project_dir: options.project_dir.clone(),
            }),
        }
    }

    fn cmd(&self, name: impl AsRef<OsStr>) -> Command {
        let mut cmd = Command::new(name);
        if let Some(path) = &self.inner.project_dir {
            cmd.current_dir(path);
        }
        cmd
    }

    pub fn build(&self) -> Result<BuildResult> {
        let inner = &self.inner;

        if let Verify::None = inner.verify {
            return Ok(BuildResult {
                reproduces_issue: false,
                no_verify: true,
                output: String::new(),
                allow_color: inner.allow_color,
            });
        }

        let (is_ice, output) = match &inner.mode {
            BuildMode::Cargo { args } => {
                let mut cmd = self.cmd("cargo");
                cmd.arg("build");

                if inner.allow_color {
                    cmd.arg("--color=always");
                }

                for arg in args.iter().flatten() {
                    cmd.arg(arg);
                }

                for env in &inner.env {
                    cmd.env(&env.key, &env.value);
                }

                let outputs = cmd.output().context("spawning rustc process")?;

                let output = String::from_utf8(outputs.stderr)?;

                (
                    // Cargo always exits with 101 when rustc has an error.
                    output.contains("internal compiler error") || output.contains("' panicked at"),
                    output,
                )
            }
            BuildMode::Script(script_path) => {
                let mut cmd = self.cmd(script_path);

                for env in &inner.env {
                    cmd.env(&env.key, &env.value);
                }

                let outputs = cmd.output().context("spawning script")?;

                let output = String::from_utf8(outputs.stderr)?;

                (outputs.status.success(), output)
            }
            BuildMode::Rustc => {
                let mut cmd = self.cmd("rustc");
                cmd.args(["--edition", "2021"]);
                cmd.arg(&inner.input_path);

                if inner.allow_color {
                    cmd.arg("--color=always");
                }

                for env in &inner.env {
                    cmd.env(&env.key, &env.value);
                }

                let outputs = cmd.output().context("spawning rustc process")?;

                let output = String::from_utf8(outputs.stderr)?;

                (
                    outputs.status.code() == Some(101)
                        || output.contains("internal compiler error"),
                    output,
                )
            }
        };

        let reproduces_issue = match inner.verify {
            Verify::None => unreachable!("handled ealier"),
            Verify::Ice => is_ice,
            Verify::Custom(func) => func.call(&output),
        };

        Ok(BuildResult {
            reproduces_issue,
            no_verify: false,
            output,
            allow_color: inner.allow_color,
        })
    }

    pub fn get_diags(&self) -> Result<(Vec<Diagnostic>, Vec<rustfix::Suggestion>)> {
        let inner = &self.inner;

        let diags = match &inner.mode {
            BuildMode::Cargo { args } => {
                let mut cmd = self.cmd("cargo");
                cmd.args(["build", "--message-format=json"]);

                for arg in args.iter().flatten() {
                    cmd.arg(arg);
                }

                for env in &inner.env {
                    cmd.env(&env.key, &env.value);
                }

                let cmd_output = cmd.output()?;
                let output = String::from_utf8(cmd_output.stdout)?;

                let messages = serde_json::Deserializer::from_str(&output)
                    .into_iter::<CargoJsonCompileMessage>()
                    .collect::<Result<Vec<_>, _>>()?;

                messages
                    .into_iter()
                    .filter(|msg| msg.reason == "compiler-message")
                    .flat_map(|msg| msg.message)
                    .collect()
            }
            BuildMode::Rustc => {
                let mut cmd = self.cmd("rustc");
                cmd.args(["--edition", "2021", "--error-format=json"]);
                cmd.arg(&inner.input_path);

                for env in &inner.env {
                    cmd.env(&env.key, &env.value);
                }

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
    output: String,
    allow_color: bool,
}

impl BuildResult {
    pub fn require_reproduction(&self, build: &str) -> Result<()> {
        if !self.reproduces_issue() {
            bail!(
                "{build} build must reproduce issue. Output:\n{}",
                self.output
            );
        }
        Ok(())
    }

    pub fn reproduces_issue(&self) -> bool {
        self.reproduces_issue || self.no_verify
    }
}

impl Display for BuildResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use owo_colors::OwoColorize;

        match self.allow_color {
            false => match (self.reproduces_issue, self.no_verify) {
                (true, _) => f.write_str("yes"),
                (false, true) => f.write_str("yes (no-verify)"),
                (false, false) => f.write_str("no"),
            },
            true => match (self.reproduces_issue, self.no_verify) {
                (true, _) => write!(f, "{}", "yes".green()),
                (false, true) => write!(f, "{}", "yes (no-verify)".green()),
                (false, false) => write!(f, "{}", "no".red()),
            },
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CargoJsonCompileMessage {
    pub reason: String,
    pub message: Option<Diagnostic>,
}
