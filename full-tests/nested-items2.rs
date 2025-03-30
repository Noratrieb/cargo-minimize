// this should all get deleted in a single swoop *and* not panic about it
/// ~REQUIRE-DELETED l1
mod l1 {
    mod l2 {
        mod l3 {
            mod l4{
                mod l5 {
                    fn foo(){}
                    fn bar(){}
                    mod l6 {
                        fn x1(){}
                    }
                    fn x2(){}
                }
            }
            mod l4_2 {
                fn y(){}
            }
        }
    }
    fn x8(){}
}
/// ~MINIMIZE-ROOT main
fn main(){}
