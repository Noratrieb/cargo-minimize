use anyhow::Result;

fn run_test(_: &str) -> Result<()> {
    let _ = tempfile::tempdir()?;
    Ok(())
}

#[test]
fn smoke() {
    run_test("").unwrap();
}
