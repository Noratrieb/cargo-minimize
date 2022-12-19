#[macro_use]
extern crate tracing;

use std::{path::PathBuf, str::FromStr};

mod build;
mod everybody_loops;
mod expand;
mod privatize;
mod processor;

use anyhow::{Context, Result};
use clap::Parser;
use processor::Minimizer;

use crate::{processor::Processor};

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

    /*
    let file = expand::expand(&dir).context("during expansion")?;

    //let file = syn::parse_str("extern { pub fn printf(format: *const ::c_char, ...) -> ::c_int; }",).unwrap();
    let file = prettyplease::unparse(&file);

    println!("// EXPANDED-START\n\n{file}\n\n// EXPANDED-END");

    std::fs::write("expanded.rs", file)?;

    println!("wow, expanded");
    */

    /*
    let build = Build::new(cargo_dir);

    if build.build()?.success {
        bail!("build must initially fail!");
    }
    */

    Ok(())
}
