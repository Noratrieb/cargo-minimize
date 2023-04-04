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
    lint_mode: BuildMode,
    input_path: PathBuf,
    verify: Verify,
    env: Vec<EnvVar>,
    allow_color: bool,
    project_dir: Option<PathBuf>,
    extra_args: Vec<String>,
}

#[derive(Debug)]
enum BuildMode {
    Cargo {
        /// May be something like `miri run`.
        subcommand: Vec<String>,
    },
    Script(PathBuf),
    Rustc,
}

impl Build {
    pub fn new(options: &Options) -> Result<Self> {
        if options.rustc && options.cargo_subcmd != "build" {
            bail!("Cannot specify --rustc together with --cargo-subcmd or --cargo-args");
        }

        let extra_args = options
            .extra_args
            .as_deref()
            .map(split_args)
            .unwrap_or_default();

        let mode = if options.rustc {
            BuildMode::Rustc
        } else if let Some(script) = &options.script_path {
            BuildMode::Script(script.clone())
        } else {
            let subcommand = split_args(&options.cargo_subcmd);
            BuildMode::Cargo { subcommand }
        };

        let lint_mode = if options.rustc {
            BuildMode::Rustc
        } else if let Some(script) = options
            .script_path_lints
            .as_ref()
            .or(options.script_path.as_ref())
        {
            BuildMode::Script(script.clone())
        } else {
            let subcommand = options
                .cargo_subcmd_lints
                .as_deref()
                .map(split_args)
                .unwrap_or_else(|| split_args(&options.cargo_subcmd));
            BuildMode::Cargo { subcommand }
        };

        let verify = if options.no_verify {
            Verify::None
        } else if let Some(func) = options.verify_fn {
            Verify::Custom(func)
        } else {
            Verify::Ice
        };

        Ok(Self {
            inner: Rc::new(BuildInner {
                mode,
                lint_mode,
                input_path: options.path.clone(),
                verify,
                env: options.env.clone(),
                allow_color: !options.no_color,
                project_dir: options.project_dir.clone(),
                extra_args,
            }),
        })
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

        let (is_ice, cmd_status, output) = match &inner.mode {
            BuildMode::Cargo { subcommand } => {
                let mut cmd = self.cmd("cargo");

                cmd.args(subcommand);

                if inner.allow_color {
                    cmd.arg("--color=always");
                }

                cmd.args(&inner.extra_args);

                for env in &inner.env {
                    cmd.env(&env.key, &env.value);
                }

                let outputs = cmd.output().context("spawning rustc process")?;

                let output = String::from_utf8(outputs.stderr)?;

                (
                    // Cargo always exits with 101 when rustc has an error.
                    output.contains("internal compiler error") || output.contains("' panicked at"),
                    outputs.status,
                    output,
                )
            }
            BuildMode::Rustc => {
                let mut cmd = self.cmd("rustc");
                cmd.args(["--edition", "2021"]);
                cmd.arg(&inner.input_path);

                if inner.allow_color {
                    cmd.arg("--color=always");
                }

                cmd.args(&inner.extra_args);

                for env in &inner.env {
                    cmd.env(&env.key, &env.value);
                }

                let outputs = cmd.output().context("spawning rustc process")?;

                let output = String::from_utf8(outputs.stderr)?;

                (
                    outputs.status.code() == Some(101)
                        || output.contains("internal compiler error"),
                    outputs.status,
                    output,
                )
            }
            BuildMode::Script(script_path) => {
                let mut cmd = self.cmd(script_path);

                cmd.args(&inner.extra_args);

                for env in &inner.env {
                    cmd.env(&env.key, &env.value);
                }

                let outputs = cmd
                    .output()
                    .with_context(|| format!("spawning script: `{cmd:?}`"))?;

                let output = String::from_utf8(outputs.stderr)?;

                (outputs.status.success(), outputs.status, output)
            }
        };

        let reproduces_issue = match inner.verify {
            Verify::None => unreachable!("handled ealier"),
            Verify::Ice => is_ice,
            Verify::Custom(func) => func.call(&output, cmd_status.code()),
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

        fn grab_cargo_diags(output: &str) -> Result<Vec<Diagnostic>> {
            let messages = serde_json::Deserializer::from_str(output)
                .into_iter::<CargoJsonCompileMessage>()
                .collect::<Result<Vec<_>, _>>()?;

            Ok(messages
                .into_iter()
                .filter(|msg| msg.reason == "compiler-message")
                .flat_map(|msg| msg.message)
                .collect())
        }

        fn grab_rustc_diags(output: &str) -> Result<Vec<Diagnostic>> {
            serde_json::Deserializer::from_str(&output)
                .into_iter::<Diagnostic>()
                .collect::<Result<_, _>>()
                .map_err(Into::into)
        }

        let diags = match &inner.lint_mode {
            BuildMode::Cargo { subcommand } => {
                let mut cmd = self.cmd("cargo");

                cmd.args(subcommand);

                cmd.arg("--message-format=json");

                cmd.args(&inner.extra_args);

                for env in &inner.env {
                    cmd.env(&env.key, &env.value);
                }

                let cmd_output = cmd.output()?;
                let output = String::from_utf8(cmd_output.stdout)?;

                grab_cargo_diags(&output)?
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

                grab_rustc_diags(&output)?
            }
            BuildMode::Script(script_path) => {
                let mut cmd = self.cmd(script_path);

                cmd.args(&inner.extra_args);

                for env in &inner.env {
                    cmd.env(&env.key, &env.value);
                }

                cmd.env("MINIMIZE_LINTS", "1");

                let outputs = cmd
                    .output()
                    .with_context(|| format!("spawning script: `{cmd:?}`"))?;

                let stderr = String::from_utf8(outputs.stderr)?;
                let stdout = String::from_utf8(outputs.stdout)?;

                let (output, mode) = read_script_output(&stdout, &stderr);

                match mode {
                    LintMode::Rustc => grab_rustc_diags(output)?,
                    LintMode::Cargo => grab_cargo_diags(output)?,
                }
            }
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

#[derive(Debug)]
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

fn split_args(s: &str) -> Vec<String> {
    s.split_whitespace().map(ToString::to_string).collect()
}

#[derive(Debug, PartialEq, Eq)]
enum LintMode {
    Rustc,
    Cargo,
}

fn read_script_output<'a>(stdout: &'a str, stderr: &'a str) -> (&'a str, LintMode) {
    let is_marked_output = |output: &str| {
        let first_line = output.lines().next();
        match first_line {
            None => None,
            Some(line) if line.contains("minimize-fmt-cargo") => Some(LintMode::Cargo),
            Some(line) if line.contains("minimize-fmt-rustc") => Some(LintMode::Rustc),
            Some(_) => None,
        }
    };

    is_marked_output(stdout)
        .map(|mode| (stdout, mode))
        .or(is_marked_output(stderr).map(|mode| (stderr, mode)))
        .unwrap_or_else(|| (stdout, LintMode::Cargo))
}

#[cfg(test)]
mod tests {
    use crate::build::LintMode;

    use super::read_script_output;

    #[test]
    fn script_output_default() {
        let (output, mode) = read_script_output("uwu", "owo");
        assert_eq!(output, "uwu");
        assert_eq!(mode, LintMode::Cargo);
    }

    #[test]
    fn script_output_rustc_stderr() {
        let (output, mode) = read_script_output("wrong", "minimize-fmt-rustc");
        assert_eq!(output, "minimize-fmt-rustc");
        assert_eq!(mode, LintMode::Rustc);
    }

    #[test]
    fn script_output_cargo_stderr() {
        let (output, mode) = read_script_output("wrong", "minimize-fmt-cargo");
        assert_eq!(output, "minimize-fmt-cargo");
        assert_eq!(mode, LintMode::Cargo);
    }

    #[test]
    fn script_output_rustc_stdout() {
        let (output, mode) = read_script_output("minimize-fmt-rustc", "wrong");
        assert_eq!(output, "minimize-fmt-rustc");
        assert_eq!(mode, LintMode::Rustc);
    }
}
