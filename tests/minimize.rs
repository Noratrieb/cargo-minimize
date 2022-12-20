mod helper;

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
