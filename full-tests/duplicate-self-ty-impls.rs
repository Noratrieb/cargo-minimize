trait A {}
/// ~REQUIRE-DELETED trait-B
trait B {}
trait C {}

/// ~MINIMIZE-ROOT impl-A
impl A for () {}

/// ~REQUIRE-DELETED impl-B
impl B for () {}

/// ~MINIMIZE-ROOT impl-C
impl C for () {}

/// ~MINIMIZE-ROOT main
fn main() {}
