use syn::{parse_quote, visit_mut::VisitMut, Visibility};

use crate::processor::Processor;

struct Visitor {
    pub_crate: Visibility,
    has_made_change: bool,
}

impl Visitor {
    fn new() -> Self {
        Self {
            has_made_change: false,
            pub_crate: parse_quote! { pub(crate) },
        }
    }
}

impl VisitMut for Visitor {
    fn visit_visibility_mut(&mut self, vis: &mut Visibility) {
        if let Visibility::Public(_) = vis {
            self.has_made_change = true;
            *vis = self.pub_crate.clone();
        }
    }
}

#[derive(Default)]
pub struct Privarize {}

impl Processor for Privarize {
    fn process_file(&mut self, krate: &mut syn::File) -> bool {
        let mut visitor = Visitor::new();
        visitor.visit_file_mut(krate);
        visitor.has_made_change
    }

    fn name(&self) -> &'static str {
        "privatize"
    }
}
