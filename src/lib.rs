#![allow(dead_code)]

use std::path::PathBuf;

mod build;
mod everybody_loops;
mod expand;
mod privatize;
mod processor;

use anyhow::Result;
use clap::Parser;
use processor::Minimizer;

use crate::{everybody_loops::EverybodyLoops, processor::Processor};

#[derive(clap::Parser)]
pub struct Options {
    #[arg(short, long)]
    verify_error_path: Option<PathBuf>,
    #[arg(long)]
    cargo: bool,
    path: PathBuf,
}

pub fn minimize() -> Result<()> {
    let options = Options::parse();

    let build = build::Build::new(&options);

    let mut minimizer = Minimizer::new_glob_dir(&options.path, build);

    println!("{minimizer:?}");

    minimizer.run_passes([
        //Box::new(Privarize::default()) as Box<dyn Processor>,
        Box::new(EverybodyLoops::default()) as Box<dyn Processor>,
    ])?;

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
