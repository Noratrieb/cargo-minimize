mod A {
    /// ~REQUIRE-DELETED S1
    pub struct S1;
    pub struct S2;
}

mod B {
    use crate::A::{self, S1};
    pub use A::S2 as thingy;
}

fn main() {
    "~MINIMIZE-ROOT let x = B::thingy";
    let x = B::thingy;
}
