use anyhow::{ensure, Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::fs::Permissions;
use std::io::BufWriter;
use std::os::unix::prelude::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::{
    fs,
    io::{self, Write},
    path::Path,
};
use tempfile::TempDir;

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

        build(&path).with_context(|| format!("building {:?}", path.file_name().unwrap()))?;
    }

    Ok(())
}

fn setup_dir(path: &Path) -> Result<(TempDir, PathBuf)> {
    let proj_dir = path.file_name().unwrap().to_str().unwrap();

    writeln!(io::stdout(), ".... Testing {}", proj_dir)?;

    let tempdir = tempfile::tempdir()?;

    fs_extra::copy_items(&[path], &tempdir, &fs_extra::dir::CopyOptions::new())?;

    let proj_dir = tempdir.path().join(proj_dir).canonicalize()?;

    Ok((tempdir, proj_dir))
}

fn setup_scripts(start_roots: &[String], proj_dir: &Path) -> Result<()> {
    // FIXME: Do this in a good way.
    // What the fuck is this.
    {
        let file = fs::File::create(proj_dir.join("check.sh"))?;

        let expected_roots = start_roots
            .iter()
            .map(|root| format!("'{}'", root))
            .collect::<Vec<_>>()
            .join(", ");

        write!(
            BufWriter::new(&file),
            r#"#!/usr/bin/env bash

OUT=$(rg -o "~MINIMIZE-ROOT [\w\-]*" full-tests/ --no-filename --sort path src)
        
python3 -c "
out = '$OUT'
        
lines = out.split('\n')
        
found = set()
        
for line in lines:
    name = line.removeprefix('~MINIMIZE-ROOT').strip()
    found.add(name)
        
    expected_roots = {{{expected_roots}}}
        
    for root in expected_roots:
        if root in found:
            print(f'Found {{root}} in output')
        else:
            print(f'Did not find {{root}} in output!')
            exit(1)
"
        "#
        )?;

        file.set_permissions(Permissions::from_mode(0o777))?;
    }
    {
        let file = fs::File::create(proj_dir.join("lint.sh"))?;

        write!(
            BufWriter::new(&file),
            r#"#!/usr/bin/env bash
cargo check
        "#
        )?;

        file.set_permissions(Permissions::from_mode(0o777))?;
    }
    Ok(())
}

fn build(path: &Path) -> Result<()> {
    let (_tempdir, proj_dir) = setup_dir(path).context("setting up tempdir")?;
    let cargo_minimize = Path::new("target/debug/cargo-minimize")
        .canonicalize()
        .context("canonicalizing target/debug/cargo-minimize")?;

    let start_roots = get_roots(&proj_dir).context("getting initial MINIMIZE-ROOTs")?;
    eprintln!("Roots: {:?}", start_roots);

    setup_scripts(&start_roots, &proj_dir).context("setting up scripts")?;

    let mut cmd = Command::new(cargo_minimize);
    cmd.current_dir(&proj_dir);

    cmd.arg("minimize");
    cmd.arg("--script-path=./check.sh");
    cmd.arg("--script-path-lints=./lint.sh");

    let out = cmd.output().context("spawning cargo-minimize")?;
    let stderr = String::from_utf8(out.stderr).unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();

    ensure!(
        out.status.success(),
        "Command failed:\n--- stderr:\n{stderr}\n--- stdout:\n{stdout}"
    );

    let required_deleted = get_required_deleted(&proj_dir).context("get REQUIRED-DELETED")?;

    ensure!(
        required_deleted.is_empty(),
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
    let path = path.join("src");
    let mut results = Vec::new();
    let walk = walkdir::WalkDir::new(path);

    for entry in walk {
        let entry = entry?;
        if !entry.metadata()?.is_file() {
            continue;
        }
        let src = fs::read_to_string(entry.path()).context("reading file")?;
        let captures = regex.captures_iter(&src);
        for cap in captures {
            let root_name = cap.get(1).unwrap();
            results.push(root_name.as_str().to_owned());
        }
    }

    Ok(results)
}
