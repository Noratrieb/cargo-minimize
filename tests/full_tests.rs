use anyhow::{ensure, Result};
use std::process::Command;

#[test]
#[ignore = "FIXME: Make this not cursed."]
#[cfg(not(unix))]
fn full_tests() -> Result<()> {
    todo!()
}

#[test]
#[cfg(unix)]
fn full_tests() -> Result<()> {
    let status = Command::new("cargo").arg("runtest").spawn()?.wait()?;
    ensure!(status.success(), "runtest failed");
    Ok(())
}
