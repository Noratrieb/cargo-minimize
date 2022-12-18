use quote::ToTokens;
use syn::{parse_quote, visit_mut::VisitMut};

use crate::processor::{tracking, PassController, ProcessState, Processor, SourceFile};

struct Visitor<'a> {
    current_path: Vec<String>,
    checker: &'a mut PassController,

    loop_expr: syn::Block,
    process_state: ProcessState,
}

impl<'a> Visitor<'a> {
    fn new(checker: &'a mut PassController) -> Self {
        Self {
            current_path: Vec::new(),
            checker,
            process_state: ProcessState::NoChange,
            loop_expr: parse_quote! { { loop {} } },
        }
    }
}

impl VisitMut for Visitor<'_> {
    fn visit_block_mut(&mut self, block: &mut syn::Block) {
        match block.stmts.as_slice() {
            [syn::Stmt::Expr(syn::Expr::Loop(syn::ExprLoop {
                body: loop_body, ..
            }))] if loop_body.stmts.is_empty() => {}
            _ if self.checker.can_process(&self.current_path) => {
                *block = self.loop_expr.clone();
                self.process_state = ProcessState::Changed;
            }
            _ => {}
        }
    }

    tracking!();
}

#[derive(Default)]
pub struct EverybodyLoops;

impl Processor for EverybodyLoops {
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
        "everybody-loops"
    }
}
