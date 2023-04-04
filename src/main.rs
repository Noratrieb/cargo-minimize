#[macro_use]
extern crate tracing;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use cargo_minimize::{Cargo, Parser};
use tracing::{error, Level};

fn main() {
    let Cargo::Minimize(options) = Cargo::parse();

    cargo_minimize::init_recommended_tracing_subscriber(Level::INFO);

    let cancel = Arc::new(AtomicBool::new(false));
    let cancel2 = Arc::clone(&cancel);

    let mut ctrl_c_pressed = false;
    let result = ctrlc::set_handler(move || {
        // If ctrl c was pressed already, kill it now.
        if ctrl_c_pressed {
            warn!("Process killed");
            std::process::exit(130);
        }

        warn!("Shutting down gracefully, press CTRL-C again to kill");
        cancel.store(true, Ordering::SeqCst);
        ctrl_c_pressed = true;
    });

    if let Err(err) = result {
        error!("Failed to install CTRL-C handler: {err}");
    }

    if let Err(err) = cargo_minimize::minimize(options, cancel2) {
        error!("An error occured:\n{err}");
        std::process::exit(1);
    }
}
