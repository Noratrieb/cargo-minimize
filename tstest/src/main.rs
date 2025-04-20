use tree_sitter::{Node, Parser, Tree};

fn main() {
    let mut parser = Parser::new();
    parser.set_language(tree_sitter_rust::language()).unwrap();
    let src = "/// hello\nstruct A (pub(crate) u8,u8);";
    let result = parser.parse(src, None).unwrap();

    print_node(src, 0, result.root_node());

    println!("{}", result.root_node().to_sexp());
}

fn print_node(src: &str, ind: usize, node: Node<'_>) {
    println!(
        "{}{} `{}`",
        " ".repeat(ind),
        node.kind(),
        src[node.byte_range()].replace("\n", "\\n")
    );

    for i in 0..node.child_count() {
        print_node(src, ind + 1, node.child(i).unwrap())
    }
}
