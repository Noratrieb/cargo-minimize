#[macro_use]
extern crate tracing;

use std::{
    path::PathBuf,
    str::FromStr,
    sync::{atomic::AtomicBool, Arc},
};

mod build;
mod dylib_flag;
mod passes;
mod processor;

#[cfg(this_pulls_in_cargo_which_is_a_big_dep_i_dont_like_it)]
mod expand;

use anyhow::{Context, Result};
use dylib_flag::RustFunction;
use processor::Minimizer;
use tracing::Level;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};

use crate::processor::Pass;

// Export so that the user doesn't have to add clap themselves.
pub use clap::Parser;

#[derive(clap::Parser)]
#[command(version, about, name = "cargo", bin_name = "cargo")]
pub enum Cargo {
    Minimize(Options),
}

#[derive(clap::Args, Debug, Clone)]
pub struct Options {
    /// Additional arguments to pass to cargo/rustc, separated by whitespace.
    #[arg(long)]
    pub extra_args: Option<String>,

    /// The cargo subcommand used to find the reproduction, seperated by whitespace (for example `miri run`).
    #[arg(long, default_value = "build")]
    pub cargo_subcmd: String,

    /// The cargo subcommand used to get diagnostics like the dead_code lint from the compiler, seperated by whitespace.
    /// Defaults to the value of `--cargo-subcmd`.
    #[arg(long)]
    pub cargo_subcmd_lints: Option<String>,

    /// To disable colored output.
    #[arg(long)]
    pub no_color: bool,

    /// This option bypasses cargo and uses rustc directly. Only works when a single file is passed as an argument.
    #[arg(long)]
    pub rustc: bool,

    /// Skips testing whether the regression reproduces and just does the most aggressive minimization. Mostly useful
    /// for testing and demonstration purposes.
    #[arg(long)]
    pub no_verify: bool,

    /// A Rust closure returning a bool that checks whether a regression reproduces.
    /// Example: `--verify-fn='|output| output.contains("internal compiler error")'`
    #[arg(long)]
    pub verify_fn: Option<RustFunction>,

    /// Additional environment variables to pass to cargo/rustc.
    /// Example: `--env NAME=VALUE --env ANOTHER_NAME=VALUE`
    #[arg(long)]
    pub env: Vec<EnvVar>,

    /// The working directory where cargo/rustc are invoked in. By default, this is the current working directory.
    #[arg(long)]
    pub project_dir: Option<PathBuf>,

    /// The directory/file of the code to be minimized.
    #[arg(default_value = "src")]
    pub path: PathBuf,

    /// A comma-seperated list of passes that should be enabled. By default, all passes are enabled.
    #[arg(long)]
    pub passes: Option<String>,

    /// A path to a script that is run to check whether code reproduces. When it exits with code 0, the
    /// problem reproduces. If `--script-path-lints` isn't set, this script is also run to get lints.
    /// For lints, the `MINIMIZE_LINTS` environment variable will be set to `1`.
    /// The first line of the lint stdout or stderr can be `minimize-fmt-rustc` or `minimize-fmt-cargo` to show whether the rustc or wrapper cargo
    /// lint format and which output stream is used. Defaults to cargo and stdout.
    #[arg(long)]
    pub script_path: Option<PathBuf>,

    /// A path to a script that is run to get lints.
    /// The first line of stdout or stderr must be `minimize-fmt-rustc` or `minimize-fmt-cargo` to show whether the rustc or wrapper cargo
    /// lint format and which output stream is used. Defaults to cargo and stdout.
    #[arg(long)]
    pub script_path_lints: Option<PathBuf>,

    /// Do not touch the following files.
    #[arg(long)]
    pub ignore_file: Vec<PathBuf>,

    #[arg(skip)]
    pub no_delete_functions: bool,
}

#[derive(Debug, Clone)]
pub struct EnvVar {
    pub key: String,
    pub value: String,
}

impl FromStr for EnvVar {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut split = s.split('=');
        let key = split
            .next()
            .ok_or("env var must have KEY=VALUE format")?
            .to_string();
        let value = split
            .next()
            .ok_or("env var must have KEY=VALUE format")?
            .to_string();
        Ok(Self { key, value })
    }
}

pub fn minimize(options: Options, stop: Arc<AtomicBool>) -> Result<()> {
    for ignore_file in &options.ignore_file {
        if !ignore_file.try_exists()? {
            warn!("Ignored path {} does not exist", ignore_file.display());
        }
    }

    let build = build::Build::new(&options)?;

    let mut minimizer = Minimizer::new_glob_dir(options, build, stop)?;

    minimizer.run_passes([
        passes::Privatize::default().boxed(),
        passes::EverybodyLoops::default().boxed(),
        passes::FieldDeleter::default().boxed(),
        passes::ItemDeleter::default().boxed(),
    ])?;

    minimizer.delete_dead_code().context("deleting dead code")?;

    Ok(())
}

pub fn init_recommended_tracing_subscriber(default_level: Level) {
    let registry = Registry::default().with(
        EnvFilter::builder()
            .with_default_directive(default_level.into())
            .from_env()
            .unwrap(),
    );

    let tree_layer = tracing_tree::HierarchicalLayer::new(2)
        .with_targets(true)
        .with_bracketed_fields(true);

    registry.with(tree_layer).init();
}

impl Default for Options {
    fn default() -> Self {
        Self {
            extra_args: None,
            cargo_subcmd: "build".into(),
            cargo_subcmd_lints: None,
            no_color: false,
            rustc: false,
            no_verify: false,
            verify_fn: None,
            env: Vec::new(),
            project_dir: None,
            path: PathBuf::from("/the/wrong/path/you/need/to/change/it"),
            passes: None,
            script_path: None,
            script_path_lints: None,
            ignore_file: Vec::new(),
            no_delete_functions: false,
        }
    }
}
