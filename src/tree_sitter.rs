use anyhow::{Context, Result};

pub fn parse(source: &str) -> Result<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(tree_sitter_rust::language())
        .context("loading tree sitter rust grammar")?;
    let content_ts = parser.parse(source, None).context("parsing file")?;
    Ok(content_ts)
}

pub fn format(file: tree_sitter::Tree, source: &str) -> anyhow::Result<String> {
    let mut s = Vec::new();
    tree_sitter_edit::render(&mut s, &file, source.as_bytes(), &Editor);

    Ok(String::from_utf8(s).unwrap())
}

struct Editor;

impl tree_sitter_edit::Editor for Editor {
    fn has_edit(&self, tree: &tree_sitter::Tree, node: &tree_sitter::Node<'_>) -> bool {
        false
    }

    fn edit(
        &self,
        source: &[u8],
        tree: &tree_sitter::Tree,
        node: &tree_sitter::Node<'_>,
    ) -> Vec<u8> {
        unimplemented!()
    }
}
