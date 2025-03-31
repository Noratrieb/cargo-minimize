use anyhow::{ensure, Context, Result};
use once_cell::sync::Lazy;
use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use regex::Regex;
use std::collections::hash_map::RandomState;
use std::collections::HashSet;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;
use std::{
    fs,
    io::{self, Write},
    path::Path,
};
use tempfile::TempDir;

/// This is called by the regression_checked binary during minimization and by the test runner at the end.
pub fn ensure_roots_kept(
    proj_dir: &Path,
    start_roots: impl IntoIterator<Item = impl AsRef<str>>,
) -> Result<()> {
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

fn run_build(cargo: &Path, command: &mut Command) -> Result<()> {
    command.env("MINIMIZE_CARGO", cargo);
    let exit = command
        .spawn()
        .context("failed to spawn command")?
        .wait()
        .context("failed to wait for command")?;

    ensure!(exit.success(), "command failed");
    Ok(())
}

pub fn full_tests() -> Result<()> {
    let cargo = cargo_minimize::rustup_which("cargo")?;

    run_build(
        &cargo,
        Command::new(&cargo).args([
            "build",
            "-p",
            "cargo-minimize",
            "-p",
            "testsuite",
            "--bin",
            "regression_checker",
            "--bin",
            "cargo-minimize",
        ]),
    )
    .context("running cargo build")?;

    let this_file = Path::new(file!())
        .canonicalize()
        .with_context(|| format!("failed to find current file: {}", file!()))?;

    let root_dir = this_file
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let full_tests_path = root_dir.join("full-tests");

    let mut regression_checker_path = root_dir
        .join("target")
        .join("debug")
        .join("regression_checker");

    if cfg!(windows) {
        regression_checker_path.set_extension("exe");
    }

    let children = fs::read_dir(&full_tests_path)
        .with_context(|| format!("reading {}", full_tests_path.display()))?;

    let children = children
        .map(|e| e.map_err(Into::into))
        .collect::<Result<Vec<_>>>()?;

    if std::env::var("PARALLEL").as_deref() != Ok("0") {
        children
            .into_par_iter()
            .map(|child| {
                let path = child.path();

                build(&cargo, &path, &regression_checker_path)
                    .with_context(|| format!("building {:?}", path.file_name().unwrap()))
            })
            .collect::<Result<Vec<_>>>()?;
    } else {
        for child in children {
            let path = child.path();

            build(&cargo, &path, &regression_checker_path)
                .with_context(|| format!("building {:?}", path.file_name().unwrap()))?;
        }
    }

    Ok(())
}

fn setup_dir(cargo: &Path, path: &Path) -> Result<(TempDir, PathBuf)> {
    let tempdir = tempfile::tempdir()?;

    let proj_name = path.file_name().unwrap().to_str().unwrap();
    let proj_name = if let Some(proj_name) = proj_name.strip_suffix(".rs") {
        let out = Command::new(cargo)
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

fn build(cargo: &Path, path: &Path, regression_checker_path: &Path) -> Result<()> {
    let (_tempdir, proj_dir) = setup_dir(cargo, path).context("setting up tempdir")?;
    let mut cargo_minimize_path = PathBuf::from("target/debug/cargo-minimize");
    if cfg!(windows) {
        cargo_minimize_path.set_extension("exe");
    }
    let cargo_minimize = cargo_minimize_path
        .canonicalize()
        .context("canonicalizing target/debug/cargo-minimize")?;

    let start_roots = get_roots(&proj_dir).context("getting initial MINIMIZE-ROOTs")?;

    let mut cmd = Command::new(cargo_minimize);
    cmd.current_dir(&proj_dir);

    cmd.arg("minimize");
    cmd.arg({
        let mut flag = OsString::from("--script-path=");
        flag.push(regression_checker_path);
        flag
    });
    cmd.arg("--bisect-delete-imports");

    let minimize_roots = start_roots.join(",");

    cmd.env("MINIMIZE_RUNTEST_ROOTS", &minimize_roots);
    cmd.env("MINIMIZE_CARGO", cargo);

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

    ensure_roots_kept(&proj_dir, &start_roots)?;

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
