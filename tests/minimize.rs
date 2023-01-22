mod helper;

use std::path::Path;

use anyhow::Result;

use helper::run_test;

#[test]
fn hello_world_no_verify() -> Result<()> {
    run_test(
        r##"
    fn main() {
        println!("Hello, world!");
    }
    "##,
        r##"
    fn main() {
        loop {}
    }
    "##,
        |opts| {
            opts.no_verify = true;
        },
    )
}

#[test]
fn unused() -> Result<()> {
    // After everybody_loops, `unused` becomes dead and should be removed.
    run_test(
        r##"
        fn unused() {}

        fn main() {
            unused();
        }
    "##,
        r##"
    fn main() {
        loop {}
    }
    "##,
        |opts| {
            opts.no_verify = true;
        },
    )
}

#[test]
fn impls() -> Result<()> {
    // Delete unused impls
    run_test(
        r##"
        pub trait Uwu {}
        impl Uwu for () {}
        impl Uwu for u8 {}

        fn main() {}
        "##,
        r##"
        fn main() {}
        "##,
        |opts| {
            opts.no_verify = true;
        },
    )
}

#[test]
#[cfg_attr(windows, ignore)]
fn custom_script_success() -> Result<()> {
    let script_path = Path::new(file!())
        .parent()
        .unwrap()
        .join("always_success.sh")
        .canonicalize()?;

    run_test(
        r##"
        fn main() {}
    "##,
        r##"
    fn main() {}
    "##,
        |opts| {
            opts.script_path = Some(script_path);
        },
    )
}
