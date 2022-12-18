use anyhow::Result;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};

fn main() -> Result<()> {
    let registry = Registry::default().with(EnvFilter::from_default_env());

    info!("Starting cargo-minimize");

    let tree_layer = tracing_tree::HierarchicalLayer::new(2)
        .with_targets(true)
        .with_bracketed_fields(true);

    registry.with(tree_layer).init();

    cargo_minimize::minimize()?;

    Ok(())
}
