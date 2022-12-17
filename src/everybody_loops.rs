use syn::{parse_quote, visit_mut::VisitMut};

use crate::processor::Processor;

struct Visitor {
    loop_expr: syn::Block,
    has_made_change: bool,
}

impl Visitor {
    fn new() -> Self {
        Self {
            has_made_change: false,
            loop_expr: parse_quote! { { loop {} } },
        }
    }
}

impl VisitMut for Visitor {
    fn visit_block_mut(&mut self, block: &mut syn::Block) {
        match block.stmts.as_slice() {
            [syn::Stmt::Expr(syn::Expr::Loop(syn::ExprLoop {
                body: loop_body, ..
            }))] if loop_body.stmts.is_empty() => {}
            _ => {
                *block = self.loop_expr.clone();
                self.has_made_change = true;
            }
        }
    }
}

#[derive(Default)]
pub struct EverybodyLoops;

impl Processor for EverybodyLoops {
    fn process_file(&mut self, krate: &mut syn::File) -> bool {
        let mut visitor = Visitor::new();
        visitor.visit_file_mut(krate);
        visitor.has_made_change
    }

    fn name(&self) -> &'static str {
        "everybody-loops"
    }
}
