use anyhow::Result;
use tracing::{error, info, Level};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};

fn main() -> Result<()> {
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

    if let Err(err) = cargo_minimize::minimize() {
        error!("An error occured:\n{err}");
    }

    Ok(())
}
