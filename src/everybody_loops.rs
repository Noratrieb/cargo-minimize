use quote::ToTokens;
use syn::{parse_quote, visit_mut::VisitMut};

use crate::processor::{ProcessChecker, ProcessState, Processor, SourceFile};

struct Visitor<'a> {
    current_path: Vec<String>,
    checker: &'a mut ProcessChecker,

    loop_expr: syn::Block,
    process_state: ProcessState,
}

impl<'a> Visitor<'a> {
    fn new(checker: &'a mut ProcessChecker) -> Self {
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
            }))] if loop_body.stmts.is_empty() && self.checker.can_process(&self.current_path) => {}
            _ => {
                *block = self.loop_expr.clone();
                self.process_state = ProcessState::Changed;
            }
        }
    }

    fn visit_item_fn_mut(&mut self, func: &mut syn::ItemFn) {
        self.current_path.push(func.sig.ident.to_string());
        syn::visit_mut::visit_item_fn_mut(self, func);
        self.current_path.pop();
    }

    fn visit_impl_item_method_mut(&mut self, method: &mut syn::ImplItemMethod) {
        self.current_path.push(method.sig.ident.to_string());
        syn::visit_mut::visit_impl_item_method_mut(self, method);
        self.current_path.pop();
    }

    fn visit_item_impl_mut(&mut self, item: &mut syn::ItemImpl) {
        self.current_path
            .push(item.self_ty.clone().into_token_stream().to_string());
        syn::visit_mut::visit_item_impl_mut(self, item);
        self.current_path.pop();
    }

    fn visit_item_mod_mut(&mut self, module: &mut syn::ItemMod) {
        self.current_path.push(module.ident.to_string());
        syn::visit_mut::visit_item_mod_mut(self, module);
        self.current_path.pop();
    }
}

#[derive(Default)]
pub struct EverybodyLoops;

impl Processor for EverybodyLoops {
    fn process_file(
        &mut self,
        krate: &mut syn::File,
        _: &SourceFile,
        checker: &mut ProcessChecker,
    ) -> ProcessState {
        let mut visitor = Visitor::new(checker);
        visitor.visit_file_mut(krate);
        visitor.process_state
    }

    fn name(&self) -> &'static str {
        "everybody-loops"
    }
}
