use quote::ToTokens;
use syn::{visit_mut::VisitMut, Fields};

use crate::processor::{tracking, Pass, PassController, ProcessState, SourceFile, MinimizeEdit};

struct Visitor<'a> {
    current_path: Vec<String>,
    checker: &'a mut PassController,
    process_state: ProcessState,
}

impl<'a> Visitor<'a> {
    fn new(checker: &'a mut PassController) -> Self {
        Self {
            current_path: Vec::new(),
            checker,
            process_state: ProcessState::NoChange,
        }
    }

    fn consider_deleting_field(&mut self, name: String) -> bool {
        self.current_path.push(name);
        let can_process = self.checker.can_process(&self.current_path);
        if can_process {
            self.process_state = ProcessState::Changed;
        }
        self.current_path.pop();
        !can_process
    }
}

impl VisitMut for Visitor<'_> {
    fn visit_fields_mut(&mut self, fields: &mut syn::Fields) {
        match fields {
            Fields::Named(named) => {
                named.named = named
                    .named
                    .clone()
                    .into_pairs()
                    .filter(|pair| {
                        let field = pair.value();
                        self.consider_deleting_field(field.ident.as_ref().unwrap().to_string())
                    })
                    .collect();
            }
            Fields::Unnamed(unnamed) => {
                unnamed.unnamed = unnamed
                    .unnamed
                    .clone()
                    .into_pairs()
                    .enumerate()
                    .filter(|(i, _)| self.consider_deleting_field(i.to_string()))
                    .map(|(_, f)| f)
                    .collect();
            }
            Fields::Unit => {}
        }
    }

    tracking!();
}

#[derive(Default)]
pub struct FieldDeleter;

impl Pass for FieldDeleter {
    fn process_file(
        &mut self,
        krate: &mut syn::File,
        _: &SourceFile,
        checker: &mut PassController,
    ) -> ProcessState {
        let mut visitor = Visitor::new(checker);
        visitor.visit_file_mut(krate);
        visitor.process_state
    }

    fn edits_for_node(&mut self, node: tree_sitter::Node, _edits: &mut Vec<MinimizeEdit>) {
        match node.kind() {
            // Braced structs
            "field_declaration_list" => {}
            // Tuple structs
            "ordered_field_declaration_list" => {}
            _ => {}
        }
        
    }

    fn name(&self) -> &'static str {
        "field-deleter"
    }
}
