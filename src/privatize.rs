use syn::{parse_quote, visit_mut::VisitMut, Visibility};

use crate::processor::{ProcessChecker, ProcessState, Processor, SourceFile};

struct Visitor {
    pub_crate: Visibility,
    process_state: ProcessState,
}

impl Visitor {
    fn new() -> Self {
        Self {
            process_state: ProcessState::NoChange,
            pub_crate: parse_quote! { pub(crate) },
        }
    }
}

impl VisitMut for Visitor {
    fn visit_visibility_mut(&mut self, vis: &mut Visibility) {
        if let Visibility::Public(_) = vis {
            self.process_state = ProcessState::Changed;
            *vis = self.pub_crate.clone();
        }
    }
}

#[derive(Default)]
pub struct Privatize {}

impl Processor for Privatize {
    fn process_file(&mut self, krate: &mut syn::File, _: &SourceFile, _: &mut ProcessChecker) -> ProcessState {
        let mut visitor = Visitor::new();
        visitor.visit_file_mut(krate);
        visitor.process_state
    }

    fn name(&self) -> &'static str {
        "privatize"
    }
}
