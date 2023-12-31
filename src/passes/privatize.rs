use tree_sitter_edit::NodeId;

use crate::processor::{MinimizeEdit, MinimizeEditKind, Pass};

#[derive(Default)]
pub struct Privatize {}

impl Pass for Privatize {
    fn edits_for_node(&mut self, node: tree_sitter::Node, edits: &mut Vec<MinimizeEdit>) {
        if node.kind() == "visibility_modifier" {
            edits.push(MinimizeEdit {
                node_id: NodeId::new(&node),
                kind: MinimizeEditKind::DeleteNode,
            });
        }
    }

    fn name(&self) -> &'static str {
        "privatize"
    }
}
