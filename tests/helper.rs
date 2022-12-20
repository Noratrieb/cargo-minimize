use std::process::Command;

use anyhow::{bail, Result};
use cargo_minimize::Options;

fn canonicalize(code: &str) -> Result<String> {
    let ast = syn::parse_file(code)?;
    Ok(prettyplease::unparse(&ast))
}

pub fn run_test(code: &str, minimizes_to: &str, options: impl FnOnce(&mut Options)) -> Result<()> {
    let dir = tempfile::tempdir()?;

    let mut cargo = Command::new("cargo");
    cargo.args(["new", "project"]);
    cargo.current_dir(dir.path());

    let output = cargo.output()?;
    if !output.status.success() {
        bail!(
            "Failed to create cargo project, {}",
            String::from_utf8(output.stderr)?
        );
    }

    let cargo_dir = dir.path().join("project");

    let main_rs = cargo_dir.join("src/main.rs");

    std::fs::write(&main_rs, code)?;

    let mut opts = Options::default();

    let path = cargo_dir.join("src");

    opts.project_dir = Some(cargo_dir);
    opts.path = path;
    options(&mut opts);

    cargo_minimize::minimize(opts)?;

    let minimized_main_rs = std::fs::read_to_string(main_rs)?;

    let actual = canonicalize(&minimized_main_rs)?;
    let expectecd = canonicalize(minimizes_to)?;

    assert_eq!(actual, expectecd);

    Ok(())
}

