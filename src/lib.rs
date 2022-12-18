use std::path::PathBuf;

mod build;
mod everybody_loops;
mod expand;
mod privatize;
mod processor;

use anyhow::{Context, Result};
use clap::Parser;
use processor::Minimizer;

use crate::{everybody_loops::EverybodyLoops, privatize::Privatize, processor::Processor};

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

    #[arg(default_value = "src")]
    path: PathBuf,
}

pub fn minimize() -> Result<()> {
    let Cargo::Minimize(options) = Cargo::parse();

    let build = build::Build::new(&options);

    let mut minimizer = Minimizer::new_glob_dir(&options.path, build);

    minimizer.run_passes([
        Box::new(Privatize::default()) as Box<dyn Processor>,
        Box::new(EverybodyLoops::default()) as Box<dyn Processor>,
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
