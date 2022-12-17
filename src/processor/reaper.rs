//! Deletes dead code.

use crate::build::Build;

use super::{files::Changes, Minimizer, ProcessState, Processor, SourceFile};
use anyhow::{ensure, Context, Result};
use proc_macro2::Span;
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
        self.invalid = HashSet::new();
        Ok(())
    }

    fn process_file(
        &mut self,
        krate: &mut syn::File,
        file: &SourceFile,
        _: &mut super::ProcessChecker,
    ) -> ProcessState {
        assert!(
            !self.invalid.contains(file),
            "processing with invalid state"
        );

        let mut visitor = FindUnusedFunction::new(file, self.diags.iter());
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
    name: String,
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

struct FindUnusedFunction {
    unused_functions: Vec<Unused>,
    process_state: ProcessState,
}

impl FindUnusedFunction {
    fn new<'a>(file: &SourceFile, diags: impl Iterator<Item = &'a Diagnostic>) -> Self {
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

                let name = diag.message.split("`").nth(1)?.to_owned();
                let span = &diag.spans[0];

                assert_eq!(
                    span.line_start, span.line_end,
                    "encountered multiline span in dead_code"
                );

                if Path::new(&span.file_name) != file.path {
                    return None;
                }

                Some(Unused {
                    name,
                    line: span.line_start,
                    column: (span.column_start - 1)..(span.column_end - 1),
                })
            })
            .collect();

        Self {
            unused_functions,
            process_state: ProcessState::NoChange,
        }
    }

    fn should_retain_item(&mut self, span: Span) -> bool {
        let span_matches = self
            .unused_functions
            .iter()
            .map(|a| a.span_matches(span))
            .filter(|&matches| matches)
            .count();

        assert!(
            span_matches < 2,
            "multiple dead_code spans matched identifier: {span_matches}."
        );

        if span_matches == 1 {
            self.process_state = ProcessState::FileInvalidated;
        }

        span_matches == 0
    }
}

impl VisitMut for FindUnusedFunction {
    fn visit_item_impl_mut(&mut self, item_impl: &mut syn::ItemImpl) {
        item_impl.items.retain(|item| match item {
            ImplItem::Method(method) => {
                let span = method.sig.ident.span();

                self.should_retain_item(span)
            }
            _ => true,
        });

        syn::visit_mut::visit_item_impl_mut(self, item_impl);
    }

    fn visit_file_mut(&mut self, krate: &mut syn::File) {
        krate.items.retain(|item| match item {
            Item::Fn(func) => {
                let span = func.sig.ident.span();

                self.should_retain_item(span)
            }
            _ => true,
        });

        syn::visit_mut::visit_file_mut(self, krate);
    }

    fn visit_item_mod_mut(&mut self, module: &mut syn::ItemMod) {
        if let Some((_, content)) = &mut module.content {
            content.retain(|item| match item {
                Item::Fn(func) => {
                    let span = func.sig.ident.span();

                    self.should_retain_item(span)
                }
                _ => true,
            })
        }

        syn::visit_mut::visit_item_mod_mut(self, module);
    }
}
