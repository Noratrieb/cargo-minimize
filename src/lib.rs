#[macro_use]
extern crate tracing;

use std::{path::PathBuf, str::FromStr};

mod build;
mod dylib_flag;
mod everybody_loops;
mod privatize;
mod processor;

#[cfg(this_pulls_in_cargo_which_is_a_big_dep_i_dont_like_it)]
mod expand;

use anyhow::{Context, Result};
use dylib_flag::RustFunction;
use processor::Minimizer;

use crate::processor::Processor;

// Export so that the user doesn't have to add clap themselves.
pub use clap::Parser;

#[derive(clap::Parser)]
#[command(version, about, name = "cargo", bin_name = "cargo")]
pub enum Cargo {
    Minimize(Options),
}

#[derive(clap::Args, Debug)]
pub struct Options {
    #[arg(short, long)]
    pub script_path: Option<PathBuf>,

    #[arg(long)]
    pub cargo_args: Option<String>,

    #[arg(long)]
    pub no_color: bool,

    #[arg(long)]
    pub rustc: bool,
    #[arg(long)]
    pub no_verify: bool,
    #[arg(long)]
    pub verify_fn: Option<RustFunction>,

    #[arg(long)]
    pub env: Vec<EnvVar>,

    #[arg(long)]
    pub project_dir: Option<PathBuf>,

    #[arg(default_value = "src")]
    pub path: PathBuf,
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

pub fn minimize(options: Options) -> Result<()> {
    let build = build::Build::new(&options);

    let mut minimizer = Minimizer::new_glob_dir(options, build)?;

    minimizer.run_passes([
        Box::<privatize::Privatize>::default() as Box<dyn Processor>,
        Box::<everybody_loops::EverybodyLoops>::default() as Box<dyn Processor>,
    ])?;

    minimizer.delete_dead_code().context("deleting dead code")?;

    Ok(())
}

impl Default for Options {
    fn default() -> Self {
        Self {
            script_path: None,
            cargo_args: None,
            no_color: false,
            rustc: false,
            no_verify: false,
            verify_fn: None,
            env: Vec::new(),
            project_dir: None,
            path: PathBuf::from("/the/wrong/path/you/need/to/change/it"),
        }
    }
}
