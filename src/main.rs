use anyhow::Result;
use cargo_minimize::{Cargo, Parser};
use tracing::{error, info, Level};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};

fn main() -> Result<()> {
    let Cargo::Minimize(options) = Cargo::parse();

    let registry = Registry::default().with(
        EnvFilter::builder()
            .with_default_directive(Level::INFO.into())
            .from_env()
            .unwrap(),
    );

    info!("Starting cargo-minimize");

    let tree_layer = tracing_tree::HierarchicalLayer::new(2)
        .with_targets(true)
        .with_bracketed_fields(true);

    registry.with(tree_layer).init();

    if let Err(err) = cargo_minimize::minimize(options) {
        error!("An error occured:\n{err}");
    }

    Ok(())
}
