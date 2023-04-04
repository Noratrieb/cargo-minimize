use anyhow::{ensure, Context, Result};
use std::process::Command;
use std::{
    fs,
    io::{self, Write},
    path::Path,
};

#[test]
fn full_tests() -> Result<()> {
    let path = Path::new(file!())
        .canonicalize()?
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("full-tests");

    let children = fs::read_dir(path)?;

    for child in children {
        let child = child?;
        let path = child.path();

        build(&path)?;
    }

    Ok(())
}

fn build(path: &Path) -> Result<()> {
    let cargo_minimize = Path::new("target/debug/cargo-minimize").canonicalize()?;

    let proj_dir = path.file_name().unwrap().to_str().unwrap();
    writeln!(io::stdout(), ".... Testing {}", proj_dir)?;

    let tempdir = tempfile::tempdir()?;

    fs_extra::copy_items(&[path], &tempdir, &fs_extra::dir::CopyOptions::new())?;

    let proj_dir = tempdir.path().join(proj_dir).canonicalize()?;

    let mut cmd = Command::new(cargo_minimize);
    cmd.current_dir(&proj_dir);

    cmd.arg("minimize");

    let out = cmd.output().context("spawning cargo-minimize")?;
    let stderr = String::from_utf8(out.stderr).unwrap();

    // ensure!(out.status.success(), "Command failed:\n{stderr}",);

    Ok(())
}
