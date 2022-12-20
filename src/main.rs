use cargo_minimize::{Cargo, Parser};
use tracing::error;

fn main() {
    let Cargo::Minimize(options) = Cargo::parse();

    cargo_minimize::init_recommended_tracing_subscriber();

    if let Err(err) = cargo_minimize::minimize(options) {
        error!("An error occured:\n{err}");
    }
}
