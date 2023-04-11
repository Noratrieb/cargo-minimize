use anyhow::{ensure, Result};
use std::process::Command;

#[test]
fn full_tests() -> Result<()> {
    let status = Command::new("cargo").arg("runtest").spawn()?.wait()?;
    ensure!(status.success(), "runtest failed");
    Ok(())
}
