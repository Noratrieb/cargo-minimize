use anyhow::{Context, Result};

use crate::processor::{MinimizeEdit, MinimizeEditKind};

pub fn parse(source: &str) -> Result<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(tree_sitter_rust::language())
        .context("loading tree sitter rust grammar")?;
    let content_ts = parser.parse(source, None).context("parsing file")?;
    Ok(content_ts)
}

pub fn apply_edits(
    file: tree_sitter::Tree, // Taking it by value as the old tree should not be used afterwards
    source: &str,
    edits: &[MinimizeEdit],
) -> anyhow::Result<String> {
    let mut s = Vec::new();
    tree_sitter_edit::render(&mut s, &file, source.as_bytes(), &MinimizeEditor { edits })
        .context("printing tree")?;

    Ok(String::from_utf8(s).unwrap())
}

struct MinimizeEditor<'a> {
    edits: &'a [MinimizeEdit],
}

impl tree_sitter_edit::Editor for MinimizeEditor<'_> {
    fn has_edit(&self, _tree: &tree_sitter::Tree, node: &tree_sitter::Node<'_>) -> bool {
        self.edits.iter().any(|edit| edit.node_id.is(node))
    }

    fn edit(
        &self,
        _source: &[u8],
        _tree: &tree_sitter::Tree,
        node: &tree_sitter::Node<'_>,
    ) -> Vec<u8> {
        self.edits
            .iter()
            .filter(|edit| edit.node_id.is(node))
            .find_map(|edit| {
                Some({
                    match edit.kind {
                        MinimizeEditKind::DeleteNode => Vec::new(),
                    }
                })
            })
            .unwrap()
    }
}
