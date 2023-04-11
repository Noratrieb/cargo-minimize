use anyhow::{ensure, Context, Result};
use once_cell::sync::Lazy;
use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use regex::Regex;
use std::collections::hash_map::RandomState;
use std::collections::HashSet;
use std::fs::Permissions;
use std::io::BufWriter;
#[cfg(unix)]
use std::os::unix::prelude::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::{
    fs,
    io::{self, Write},
    path::Path,
};
use tempfile::TempDir;

/// This is called by the regression_checked binary during minimization and by the test runner at the end.
pub fn ensure_correct_minimization(
    proj_dir: &Path,
    start_roots: impl IntoIterator<Item = impl AsRef<str>>,
) -> Result<()> {
    let required_deleted = get_required_deleted(&proj_dir).context("get REQUIRED-DELETED")?;

    ensure!(
        required_deleted.is_empty(),
        "Some REQUIRE-DELETED have not been deleted: {required_deleted:?}"
    );

    let end_roots = HashSet::<_, RandomState>::from_iter(
        get_roots(proj_dir).context("getting final MINIMIZE-ROOTs")?,
    );
    for root in start_roots {
        let root = root.as_ref();
        ensure!(
            end_roots.contains(root),
            "{root} was not found after minimization"
        );
    }

    Ok(())
}

fn run_build(command: &mut Command) -> Result<()> {
    let exit = command
        .spawn()
        .context("failed to spawn command")?
        .wait()
        .context("failed to wait for command")?;

    ensure!(exit.success(), "command failed");
    Ok(())
}

#[cfg(not(unix))]
pub fn full_tests() -> Result<()> {
    todo!("FIXME: Make this not cursed.")
}

#[cfg(unix)]
pub fn full_tests() -> Result<()> {
    run_build(Command::new("cargo").args([
        "build",
        "-p",
        "cargo-minimize",
        "-p",
        "testsuite",
        "--bin",
        "regression_checker",
    ]))
    .context("running cargo build")?;

    let path = Path::new(file!())
        .canonicalize()?
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("full-tests");

    let children = fs::read_dir(&path).with_context(|| format!("reading {}", path.display()))?;

    let children = children
        .map(|e| e.map_err(Into::into))
        .collect::<Result<Vec<_>>>()?;

    if std::env::var("PARALLEL").as_deref() != Ok("0") {
        children
            .into_par_iter()
            .map(|child| {
                let path = child.path();

                build(&path).with_context(|| format!("building {:?}", path.file_name().unwrap()))
            })
            .collect::<Result<Vec<_>>>()?;
    } else {
        for child in children {
            let path = child.path();

            build(&path).with_context(|| format!("building {:?}", path.file_name().unwrap()))?;
        }
    }

    Ok(())
}

fn setup_dir(path: &Path) -> Result<(TempDir, PathBuf)> {
    let tempdir = tempfile::tempdir()?;

    let proj_name = path.file_name().unwrap().to_str().unwrap();
    let proj_name = if let Some(proj_name) = proj_name.strip_suffix(".rs") {
        let out = Command::new("cargo")
            .arg("new")
            .arg(proj_name)
            .current_dir(tempdir.path())
            .output()
            .context("spawning cargo new")?;

        ensure!(out.status.success(), "Failed to run cargo new");

        fs::copy(
            path,
            tempdir.path().join(proj_name).join("src").join("main.rs"),
        )
        .context("copying to main.rs")?;
        proj_name
    } else {
        proj_name
    };

    writeln!(io::stdout(), ".... Testing {}", proj_name)?;

    fs_extra::copy_items(&[path], &tempdir, &fs_extra::dir::CopyOptions::new())?;

    let proj_dir = tempdir.path().join(proj_name).canonicalize()?;

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
if ! cargo check ; then
    >&2 echo "Cargo check failed"
    exit 1
fi

OUT=$(grep -ro "~MINIMIZE-ROOT [a-zA-Z_\-]*" --no-filename src)

python3 -c "
# Get the data from bash by just substituting it in. It works!
out = '''$OUT'''
        
lines = out.split('\n')
        
found = set()
        
for line in lines:
    name = line.removeprefix('~MINIMIZE-ROOT').strip()
    found.add(name)
        
# Pass in the data _from Rust directly_. Beautiful.
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

        #[cfg(unix)]
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

    ensure_correct_minimization(&proj_dir, &start_roots)?;

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
