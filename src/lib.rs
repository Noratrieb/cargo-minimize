#![allow(dead_code)]

use std::path::Path;

mod build;
mod expand;

use anyhow::{Context, Result};

pub fn minimize(cargo_dir: &Path) -> Result<()> {
    let file = expand::expand(cargo_dir).context("during expansion")?;


    let file = prettyplease::unparse(&file);

    println!("// EXPANDED-START\n\n{file}\n\n// EXPANDED-END");

    std::fs::write("expanded.rs", file)?;

    println!("wow, expanded");
    Ok(())

    /*
    let build = Build::new(cargo_dir);

    if build.build()?.success {
        bail!("build must initially fail!");
    }
    */
}
