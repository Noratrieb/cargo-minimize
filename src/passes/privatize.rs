use quote::ToTokens;
use syn::{Visibility, parse_quote, visit_mut::VisitMut};

use crate::processor::{Pass, PassController, ProcessState, SourceFile, tracking};

struct Visitor<'a> {
    pub_crate: Visibility,
    process_state: ProcessState,
    current_path: Vec<String>,
    checker: &'a mut PassController,
}

impl<'a> Visitor<'a> {
    fn new(checker: &'a mut PassController) -> Self {
        Self {
            process_state: ProcessState::NoChange,
            pub_crate: parse_quote! { pub(crate) },
            current_path: Vec::new(),
            checker,
        }
    }
}

impl VisitMut for Visitor<'_> {
    fn visit_visibility_mut(&mut self, vis: &mut Visibility) {
        if let Visibility::Public(_) = vis {
            self.current_path.push("{{vis}}".to_string());
            if self.checker.can_process(&self.current_path) {
                self.process_state = ProcessState::Changed;
                *vis = self.pub_crate.clone();
            }
            self.current_path.pop();
        }
    }
    fn visit_item_mut(&mut self, item: &mut syn::Item) {
        match item {
            syn::Item::Use(u) => {
                if let Visibility::Public(_) = u.vis {
                    let mut path = self.current_path.clone();
                    path.push(u.to_token_stream().to_string());
                    if self.checker.can_process(&path) {
                        self.process_state = ProcessState::Changed;
                        u.vis = self.pub_crate.clone();
                    }
                    path.pop();
                }
                return; // early return; do not walk the child items
            }
            _ => {}
        }
        syn::visit_mut::visit_item_mut(self, item);
    }

    tracking!();
}

#[derive(Default)]
pub struct Privatize {}

impl Pass for Privatize {
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

    fn name(&self) -> &'static str {
        "privatize"
    }
}
