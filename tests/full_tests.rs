use anyhow::{ensure, Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::process::Command;
use std::{
    fs,
    io::{self, Write},
    path::Path,
};

#[test]
#[ignore = "unfinished"]
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

    let start_roots = get_roots(&proj_dir).context("getting initial roots")?;
    eprintln!("Roots: {:?}", start_roots);

    let mut cmd = Command::new(cargo_minimize);
    cmd.current_dir(&proj_dir);

    cmd.arg("minimize");

    let out = cmd.output().context("spawning cargo-minimize")?;
    let stderr = String::from_utf8(out.stderr).unwrap();

    ensure!(out.status.success(), "Command failed:\n{stderr}");

    let required_deleted = get_required_deleted(&proj_dir)?;

    ensure!(
        required_deleted.len() > 0,
        "Some REQUIRE-DELETED have not been deleted: {required_deleted:?}"
    );

    Ok(())
}

fn get_roots(path: &Path) -> Result<Vec<String>> {
    static REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"~MINIMIZE-ROOT ([\w\-_]+)").unwrap());

    grep(path, &REGEX)
}

fn get_required_deleted(path: &Path) -> Result<Vec<String>> {
    static REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"~REQUIRE-DELETED ([\w\-_]+)").unwrap());

    grep(path, &REGEX)
}

fn grep(path: &Path, regex: &Regex) -> Result<Vec<String>> {
    let mut results = Vec::new();
    let walk = walkdir::WalkDir::new(path);

    for entry in walk {
        let entry = entry?;
        if !entry.metadata()?.is_file() {
            continue;
        }
        let src = fs::read_to_string(entry.path())?;
        let captures = regex.captures_iter(&src);
        for cap in captures {
            let root_name = cap.get(1).unwrap();
            results.push(root_name.as_str().to_owned());
        }
    }

    Ok(results)
}
