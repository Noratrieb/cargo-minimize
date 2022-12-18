//! Deletes dead code.

use crate::build::Build;

use super::{
    files::Changes, tracking, Minimizer, PassController, ProcessState, Processor, SourceFile,
};
use anyhow::{ensure, Context, Result};
use proc_macro2::Span;
use quote::ToTokens;
use rustfix::{diagnostics::Diagnostic, Suggestion};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    path::Path,
};
use syn::{visit_mut::VisitMut, ImplItem, Item};

fn file_for_suggestion(suggestion: &Suggestion) -> &str {
    &suggestion.solutions[0].replacements[0].snippet.file_name
}

impl Minimizer {
    pub fn delete_dead_code(&mut self) -> Result<()> {
        let inital_build = self.build.build()?;
        println!("Before reaper: {}", inital_build);
        ensure!(
            inital_build.reproduces_issue(),
            "Initial build must reproduce issue"
        );

        let (diags, suggestions) = self
            .build
            .get_diags()
            .context("getting suggestions from rustc")?;

        let mut suggestions_for_file = HashMap::<_, Vec<_>>::new();
        for suggestion in &suggestions {
            suggestions_for_file
                .entry(file_for_suggestion(suggestion))
                .or_default()
                .push(suggestion);
        }

        // Always unconditionally apply unused imports.
        self.apply_unused_imports(&suggestions_for_file)?;

        self.run_passes([
            Box::new(DeleteUnusedFunctions::new(self.build.clone(), diags)) as Box<dyn Processor>,
        ])
        .context("deleting unused functions")?;

        Ok(())
    }

    fn apply_unused_imports<'a>(
        &mut self,
        suggestions: &HashMap<&str, Vec<&Suggestion>>,
    ) -> Result<()> {
        for (file, suggestions) in suggestions {
            let Some(file) = self
                .files
                .iter()
                .find(|source| source.path == Path::new(file)) else {
                    continue;
                };

            let mut changes = &mut Changes::default();

            let mut change = file.try_change(&mut changes)?;

            let desired_suggestions = suggestions
                .iter()
                .filter(|sugg| sugg.message.contains("unused import"))
                .cloned()
                .cloned()
                .collect::<Vec<_>>();

            let result = rustfix::apply_suggestions(change.before_content(), &desired_suggestions)?;

            change.write(&result)?;

            let after = self.build.build()?;

            println!("{}: After reaper: {after}", file.path.display());

            if after.reproduces_issue() {
                change.commit();
            } else {
                change.rollback()?;
            }
        }

        Ok(())
    }
}

struct DeleteUnusedFunctions {
    diags: Vec<Diagnostic>,
    build: Build,
    invalid: HashSet<SourceFile>,
}

impl DeleteUnusedFunctions {
    fn new(build: Build, diags: Vec<Diagnostic>) -> Self {
        DeleteUnusedFunctions {
            diags,
            build,
            invalid: HashSet::new(),
        }
    }
}

impl Processor for DeleteUnusedFunctions {
    fn refresh_state(&mut self) -> Result<()> {
        let (diags, _) = self.build.get_diags().context("getting diagnostics")?;
        self.diags = diags;
        self.invalid.clear();
        Ok(())
    }

    fn process_file(
        &mut self,
        krate: &mut syn::File,
        file: &SourceFile,
        checker: &mut super::PassController,
    ) -> ProcessState {
        assert!(
            !self.invalid.contains(file),
            "processing with invalid state"
        );

        let mut visitor = FindUnusedFunction::new(file, self.diags.iter(), checker);
        visitor.visit_file_mut(krate);

        if visitor.process_state == ProcessState::FileInvalidated {
            self.invalid.insert(file.clone());
        }

        visitor.process_state
    }

    fn name(&self) -> &'static str {
        "delete-unused-functions"
    }
}

#[derive(Debug)]
struct Unused {
    line: usize,
    column: Range<usize>,
}

impl Unused {
    fn span_matches(&self, ident_span: Span) -> bool {
        let (start, end) = (ident_span.start(), ident_span.end());

        assert_eq!(start.line, end.line);

        let line_matches = self.line == start.line;
        let column_matches = self.column.start <= start.column && self.column.end >= end.column;

        line_matches && column_matches
    }
}

struct FindUnusedFunction<'a> {
    unused_functions: Vec<Unused>,
    process_state: ProcessState,
    current_path: Vec<String>,
    checker: &'a mut PassController,
}

impl<'a> FindUnusedFunction<'a> {
    fn new<'b>(
        file: &SourceFile,
        diags: impl Iterator<Item = &'b Diagnostic>,
        checker: &'a mut PassController,
    ) -> Self {
        let unused_functions = diags
            .filter_map(|diag| {
                // FIXME: use `code` correctly
                if diag
                    .code
                    .as_ref()
                    .map_or(false, |code| code.code != "dead_code")
                {
                    return None;
                }

                if !diag.message.contains("function") {
                    return None;
                }

                let span = &diag.spans[0];

                assert_eq!(
                    span.line_start, span.line_end,
                    "encountered multiline span in dead_code"
                );

                if Path::new(&span.file_name) != file.path {
                    return None;
                }

                Some(Unused {
                    line: span.line_start,
                    column: (span.column_start - 1)..(span.column_end - 1),
                })
            })
            .collect();

        Self {
            unused_functions,
            process_state: ProcessState::NoChange,
            current_path: Vec::new(),
            checker,
        }
    }

    fn should_retain_item(&mut self, span: Span) -> bool {
        let span_matches = self
            .unused_functions
            .iter()
            .map(|a| a.span_matches(span))
            .filter(|&matches| matches)
            .count();

        match span_matches {
            0 => true,
            1 => {
                self.process_state = ProcessState::FileInvalidated;
                !self.checker.can_process(&self.current_path)
            }
            _ => {
                panic!("multiple dead_code spans matched identifier: {span_matches}.");
            }
        }
    }
}

impl VisitMut for FindUnusedFunction<'_> {
    fn visit_item_impl_mut(&mut self, item_impl: &mut syn::ItemImpl) {
        self.current_path
            .push(item_impl.self_ty.clone().into_token_stream().to_string());

        item_impl.items.retain(|item| match item {
            ImplItem::Method(method) => {
                self.current_path.push(method.sig.ident.to_string());

                let span = method.sig.ident.span();

                let should_retain = self.should_retain_item(span);

                self.current_path.pop();
                should_retain
            }
            _ => true,
        });

        syn::visit_mut::visit_item_impl_mut(self, item_impl);

        self.current_path.pop();
    }

    fn visit_file_mut(&mut self, krate: &mut syn::File) {
        krate.items.retain(|item| match item {
            Item::Fn(func) => {
                self.current_path.push(func.sig.ident.to_string());

                let span = func.sig.ident.span();
                let should_retain = self.should_retain_item(span);

                self.current_path.pop();
                should_retain
            }
            _ => true,
        });

        syn::visit_mut::visit_file_mut(self, krate);
    }

    fn visit_item_mod_mut(&mut self, module: &mut syn::ItemMod) {
        self.current_path.push(module.ident.to_string());

        if let Some((_, content)) = &mut module.content {
            content.retain(|item| match item {
                Item::Fn(func) => {
                    self.current_path.push(func.sig.ident.to_string());

                    let span = func.sig.ident.span();
                    let should_retain = self.should_retain_item(span);

                    self.current_path.pop();
                    should_retain
                }
                _ => true,
            })
        }

        syn::visit_mut::visit_item_mod_mut(self, module);

        self.current_path.pop();
    }

    tracking!(visit_item_fn_mut);
    tracking!(visit_impl_item_method_mut);
}
