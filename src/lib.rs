#![allow(dead_code)]

use std::path::PathBuf;

mod build;
mod everybody_loops;
mod expand;
mod privatize;
mod processor;

use anyhow::{Context, Result};
use clap::Parser;
use processor::Minimizer;

use crate::{everybody_loops::EverybodyLoops, processor::Processor, privatize::Privatize};

#[derive(clap::Parser)]
pub struct Options {
    #[arg(short, long)]
    verify_error_path: Option<PathBuf>,
    #[arg(long)]
    cargo: bool,
    #[arg(long)]
    no_verify: bool,
    path: PathBuf,
}

pub fn minimize() -> Result<()> {
    let options = Options::parse();

    let build = build::Build::new(&options);

    let mut minimizer = Minimizer::new_glob_dir(&options.path, build, &options);

    println!("{minimizer:?}");

    minimizer.delete_dead_code().context("deleting dead code")?;

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
