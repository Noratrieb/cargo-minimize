pub mod foo {
    /// ~MINIMIZE-ROOT good
    pub fn good(){}
    /// ~REQUIRE-DELETED bad
    pub fn bad(){}
}
/// ~MINIMIZE-ROOT main
fn main(){}
