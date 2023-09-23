use anyhow::bail;

fn main() -> anyhow::Result<()> {
    let cargo = std::env::var("MINIMIZE_CARGO").expect("MINIMIZE_CARGO");

    if std::env::var("MINIMIZE_LINTS").as_deref() == Ok("1") {
        std::process::Command::new(&cargo)
            .arg("check")
            .spawn()
            .unwrap()
            .wait()
            .unwrap();
    }

    let root_var =
        std::env::var("MINIMIZE_RUNTEST_ROOTS").expect("MINIMIZE_RUNTEST_ROOTS env var not found");
    let roots = root_var.split(",").collect::<Vec<_>>();

    let proj_dir = std::env::current_dir().expect("current dir not found");

    testsuite::ensure_roots_kept(&proj_dir, roots)?;

    let check = std::process::Command::new(&cargo)
        .arg("check")
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    if !check.success() {
        bail!("cargo check failed");
    }
    Ok(())
}
