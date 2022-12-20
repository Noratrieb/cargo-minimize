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
use clap::Parser;
use dylib_flag::RustFunction;
use processor::Minimizer;

use crate::processor::Processor;

#[derive(clap::Parser)]
#[command(version, about, name = "cargo", bin_name = "cargo")]
enum Cargo {
    Minimize(Options),
}

#[derive(clap::Args, Debug)]
pub struct Options {
    #[arg(short, long)]
    script_path: Option<PathBuf>,

    #[arg(long)]
    cargo_args: Option<String>,

    #[arg(long)]
    rustc: bool,
    #[arg(long)]
    no_verify: bool,
    #[arg(long)]
    verify_fn: Option<RustFunction>,

    #[arg(long)]
    env: Vec<EnvVar>,

    #[arg(default_value = "src")]
    path: PathBuf,
}

#[derive(Debug, Clone)]
struct EnvVar {
    key: String,
    value: String,
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

pub fn minimize() -> Result<()> {
    let Cargo::Minimize(options) = Cargo::parse();

    let build = build::Build::new(&options);

    let mut minimizer = Minimizer::new_glob_dir(&options.path, build);

    minimizer.run_passes([
        Box::<privatize::Privatize>::default() as Box<dyn Processor>,
        Box::<everybody_loops::EverybodyLoops>::default() as Box<dyn Processor>,
    ])?;

    minimizer.delete_dead_code().context("deleting dead code")?;

    Ok(())
}
