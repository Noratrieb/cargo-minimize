use std::path::Path;

use anyhow::{Context, Result};

fn main() -> Result<()> {
    let dir = std::env::args().nth(1).context("expected an argument")?;

    cargo_minimize::minimize(&Path::new(&dir))?;

    println!("Exit");

    Ok(())
}
