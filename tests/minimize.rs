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
