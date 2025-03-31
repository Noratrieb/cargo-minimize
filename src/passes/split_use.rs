use crate::processor::{tracking, Pass, PassController, ProcessState, SourceFile};
use quote::ToTokens;

use syn::{visit_mut::VisitMut, Item, ItemUse};

struct Visitor<'a> {
    process_state: ProcessState,
    current_path: Vec<String>,
    checker: &'a mut PassController,
}

impl<'a> Visitor<'a> {
    fn new(checker: &'a mut PassController) -> Self {
        Self {
            process_state: ProcessState::NoChange,
            current_path: Vec::new(),
            checker,
        }
    }

    // given a "some::group::{a, b::{c,d}, e}" tree, and assuming checker allows processing of (only) "some::group",
    // returns a ["some::group::a", "some::group::b::{c,d}", "some::group::e"] list of trees.
    fn expand_use_groups(&mut self, top: &syn::ItemUse, tree: &syn::UseTree) -> Vec<syn::UseTree> {
        // It would probably be nice if instead of *expanding* the whole "some::group" group, we could instead
        // *extract* individual items ("some::group::a"), but that makes code much more convoluted, sadly
        match tree {
            syn::UseTree::Path(p) => {
                self.current_path.push(p.ident.to_string());

                let out = self
                    .expand_use_groups(top, &p.tree)
                    .into_iter()
                    .map(|x| {
                        let mut new = p.clone();
                        new.tree = Box::new(x);
                        syn::UseTree::Path(new)
                    })
                    .collect();

                self.current_path.pop();
                out
            }
            syn::UseTree::Group(g) => {
                let new_trees = g
                    .items
                    .iter()
                    .map(|subtree| self.expand_use_groups(top, subtree))
                    .flatten()
                    .collect::<Vec<_>>();

                self.current_path.push("{{group}}".to_string());
                let can_process = self.checker.can_process(&self.current_path);
                self.current_path.pop();
                if can_process {
                    self.process_state = ProcessState::Changed;
                    return new_trees;
                } else {
                    // Do not expand the group.
                    // recreate the UseTree::Group item (but with new subtrees), and return a single-element list
                    let mut g = g.clone();
                    g.items.clear();
                    g.items.extend(new_trees);
                    return vec![syn::UseTree::Group(g)];
                }
            }
            _ => return vec![tree.clone()],
        }
    }

    fn visit_item_list(&mut self, items: &mut Vec<syn::Item>) {
        let mut pos = 0; // index into the `items` list
        while pos < items.len() {
            let item_use: ItemUse = {
                match &items[pos] {
                    Item::Use(u) => u.clone(),
                    _ => {
                        pos += 1; // if it's not a `use`` - simply advance to the next item
                        continue;
                    }
                }
            };

            let new_use_trees = self.expand_use_groups(&item_use, &item_use.tree);
            // decorate each of the UseTree with a `use` keyword (and any attributes inherited)
            let new_uses = new_use_trees.into_iter().map(|x| {
                let mut new = item_use.clone();
                new.tree = x;
                syn::Item::Use(new)
            });

            let step = new_uses.len();
            // replace the old use with the new uses
            items.splice(pos..pos + 1, new_uses);
            pos += step; // do not process freshly inserted items
        }
    }
}

impl VisitMut for Visitor<'_> {
    fn visit_item_mod_mut(&mut self, item_mod: &mut syn::ItemMod) {
        self.current_path.push(item_mod.ident.to_string());
        if let Some((_, items)) = &mut item_mod.content {
            self.visit_item_list(items);
        }
        syn::visit_mut::visit_item_mod_mut(self, item_mod);
        self.current_path.pop();
    }
    fn visit_file_mut(&mut self, file: &mut syn::File) {
        self.visit_item_list(&mut file.items);
        syn::visit_mut::visit_file_mut(self, file);
    }

    tracking!(visit_item_fn_mut);
    tracking!(visit_impl_item_method_mut);
    tracking!(visit_item_impl_mut);
    tracking!(visit_field_mut);
    tracking!(visit_item_struct_mut);
    tracking!(visit_item_trait_mut);
}

#[derive(Default)]
pub struct SplitUse {}

impl Pass for SplitUse {
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
        "split-use"
    }
}
