//! Deletes dead code.

use crate::build::Build;

use super::{Minimizer, Pass, PassController, ProcessState, SourceFile, files::Changes, tracking};
use anyhow::{Context, Result};
use proc_macro2::Span;
use quote::ToTokens;
use rustfix::{Suggestion, diagnostics::Diagnostic};
use std::{collections::HashMap, ops::Range, path::Path};
use syn::{ImplItem, Item, visit_mut::VisitMut};

fn file_for_suggestion(suggestion: &Suggestion) -> &Path {
    Path::new(&suggestion.solutions[0].replacements[0].snippet.file_name)
}

const PASS_NAME: &str = "delete-unused-functions";

impl Minimizer {
    pub fn delete_dead_code(&mut self) -> Result<()> {
        if !self.pass_enabled(PASS_NAME) {
            return Ok(());
        }

        let inital_build = self.build.build()?;
        info!("Before reaper: {inital_build}");

        inital_build.require_reproduction("Initial")?;

        let (diags, suggestions) = self
            .build
            .get_diags()
            .context("getting suggestions from rustc")?;

        debug!(?diags, "Got diagnostics");

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
            Box::new(DeleteUnusedFunctions::new(self.build.clone(), diags)) as Box<dyn Pass>,
        ])
        .context("deleting unused functions")?;

        Ok(())
    }

    fn apply_unused_imports(
        &mut self,
        suggestions: &HashMap<&Path, Vec<&Suggestion>>,
    ) -> Result<()> {
        for (sugg_file, suggestions) in suggestions {
            let Some(file) = self.files.iter().find(|source| {
                source.path_no_fs_interact().ends_with(sugg_file)
                    || sugg_file.ends_with(source.path_no_fs_interact())
            }) else {
                continue;
            };

            let changes = &mut Changes::default();

            let mut change = file.try_change(changes)?;

            let desired_suggestions = suggestions
                .iter()
                .filter(|sugg| sugg.message.contains("unused import"))
                .copied()
                .cloned()
                .collect::<Vec<_>>();
            if desired_suggestions.is_empty() {
                continue;
            }

            let result =
                rustfix::apply_suggestions(change.before_content().0, &desired_suggestions)?;
            anyhow::ensure!(
                result != change.before_content().0,
                "Suggestions 'applied' but no changes made??"
            );

            let result = syn::parse_file(&result).context("parsing file after rustfix")?;
            change.write(result)?;

            let after = self.build.build()?;

            info!("{file:?}: After reaper: {after}");

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
}

impl DeleteUnusedFunctions {
    fn new(build: Build, diags: Vec<Diagnostic>) -> Self {
        DeleteUnusedFunctions { diags, build }
    }
}

impl Pass for DeleteUnusedFunctions {
    fn refresh_state(&mut self) -> Result<()> {
        let (diags, _) = self.build.get_diags().context("getting diagnostics")?;
        self.diags = diags;
        Ok(())
    }

    fn process_file(
        &mut self,
        krate: &mut syn::File,
        file: &SourceFile,
        checker: &mut super::PassController,
    ) -> ProcessState {
        let mut visitor = FindUnusedFunction::new(file, self.diags.iter(), checker);
        visitor.visit_file_mut(krate);

        visitor.process_state
    }

    fn name(&self) -> &'static str {
        PASS_NAME
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

                // When the project directory is remapped, the path may be absolute or generally have some prefix.
                if !file.path_no_fs_interact().ends_with(&span.file_name) {
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
                if self.checker.can_process(&self.current_path) {
                    self.process_state = ProcessState::FileInvalidated;
                    !self.checker.can_process(&self.current_path)
                } else {
                    true
                }
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
            ImplItem::Fn(method) => {
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
            });
        }

        syn::visit_mut::visit_item_mod_mut(self, module);

        self.current_path.pop();
    }

    tracking!(visit_item_fn_mut);
    tracking!(visit_impl_item_fn_mut);
    tracking!(visit_field_mut);
    tracking!(visit_item_struct_mut);
    tracking!(visit_item_trait_mut);
}
