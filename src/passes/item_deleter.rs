use quote::ToTokens;
use syn::{
    visit_mut::VisitMut, Item, ItemConst, ItemEnum, ItemExternCrate, ItemFn, ItemMacro, ItemMacro2,
    ItemMod, ItemStatic, ItemStruct, ItemTrait, ItemTraitAlias, ItemType, ItemUnion, Signature,
};

use crate::processor::{tracking, Pass, PassController, ProcessState, SourceFile};

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

    fn should_retain_item(&mut self) -> bool {
        let can_process = self.checker.can_process(&self.current_path);
        if can_process {
            self.process_state = ProcessState::Changed;
        }
        !can_process
    }

    fn consider_deleting_item(&mut self, item: &Item) -> bool {
        match item {
            Item::Impl(impl_) => {
                self.current_path
                    .push(impl_.self_ty.clone().into_token_stream().to_string());

                let should_retain = self.should_retain_item();

                self.current_path.pop();
                should_retain
            }
            Item::Fn(_) if self.checker.options.no_delete_functions => true,
            Item::Fn(ItemFn {
                sig: Signature { ident, .. },
                ..
            })
            | Item::Struct(ItemStruct { ident, .. })
            | Item::Enum(ItemEnum { ident, .. })
            | Item::Union(ItemUnion { ident, .. })
            | Item::Const(ItemConst { ident, .. })
            | Item::Type(ItemType { ident, .. })
            | Item::Trait(ItemTrait { ident, .. })
            | Item::TraitAlias(ItemTraitAlias { ident, .. })
            | Item::Macro(ItemMacro {
                ident: Some(ident), ..
            })
            | Item::Macro2(ItemMacro2 { ident, .. })
            | Item::Static(ItemStatic { ident, .. })
            | Item::Mod(ItemMod { ident, .. })
            | Item::ExternCrate(ItemExternCrate { ident, .. }) => {
                self.current_path.push(ident.to_string());

                let should_retain = self.should_retain_item();

                self.current_path.pop();
                should_retain
            }
            Item::ForeignMod(_) => true,
            // We hope for the unused imports to show them all.
            Item::Use(_) => true,
            Item::Verbatim(_) => true,
            #[deny(non_exhaustive_omitted_patterns)]
            _ => true,
        }
    }
}

impl VisitMut for Visitor<'_> {
    fn visit_file_mut(&mut self, file: &mut syn::File) {
        file.items
            .retain_mut(|item| self.consider_deleting_item(item));

        syn::visit_mut::visit_file_mut(self, file);
    }

    fn visit_item_mod_mut(&mut self, module: &mut syn::ItemMod) {
        self.current_path.push(module.ident.to_string());

        if let Some((_, items)) = &mut module.content {
            items.retain(|item| self.consider_deleting_item(item));
        }

        syn::visit_mut::visit_item_mod_mut(self, module);
        self.current_path.pop();
    }

    tracking!(visit_item_fn_mut);
    tracking!(visit_impl_item_method_mut);
    tracking!(visit_item_impl_mut);
    tracking!(visit_field_mut);
    tracking!(visit_item_struct_mut);
}

#[derive(Default)]
pub struct ItemDeleter;

impl Pass for ItemDeleter {
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
        "item-deleter"
    }
}
