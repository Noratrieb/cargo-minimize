use anyhow::Result;

fn main() -> Result<()> {
    cargo_minimize::minimize()?;

    Ok(())
}
