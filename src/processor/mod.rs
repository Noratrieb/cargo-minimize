mod checker;
mod files;
mod reaper;

pub(crate) use self::files::SourceFile;
use crate::{build::Build, processor::files::Changes, Options};
use anyhow::{bail, Context, Result};
use owo_colors::OwoColorize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::{collections::HashSet, ffi::OsStr, fmt::Debug, sync::atomic::AtomicBool};

pub(crate) use self::checker::PassController;

pub(crate) trait Pass {
    fn refresh_state(&mut self) -> Result<()> {
        Ok(())
    }

    /// Process a file. The state of the processor might get invalidated in the process as signaled with
    /// `ProcessState::FileInvalidated`. When a file is invalidated, the minimizer will call `Processor::refersh_state`
    /// before calling the this function on the same file again.
    fn process_file(
        &mut self,
        krate: &mut syn::File,
        file: &SourceFile,
        checker: &mut PassController,
    ) -> ProcessState;

    fn name(&self) -> &'static str;

    fn boxed(self) -> Box<dyn Pass>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

impl Debug for dyn Pass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ProcessState {
    NoChange,
    Changed,
    FileInvalidated,
}

#[derive(Debug)]
pub(crate) struct Minimizer {
    files: Vec<SourceFile>,
    build: Build,
    options: Options,
    cancel: Arc<AtomicBool>,
}

impl Minimizer {
    fn pass_disabled(&self, name: &str) -> bool {
        if let Some(passes) = &self.options.passes {
            if !passes.split(",").any(|allowed| name == allowed) {
                return true;
            }
        }
        false
    }

    pub(crate) fn new_glob_dir(
        options: Options,
        build: Build,
        cancel: Arc<AtomicBool>,
    ) -> Result<Self> {
        let path = &options.path;
        let walk = walkdir::WalkDir::new(path);

        let files = walk
            .into_iter()
            .filter_map(|entry| match entry {
                Ok(entry) => Some(entry),
                Err(err) => {
                    warn!("Error during walkdir: {err}");
                    None
                }
            })
            .filter(|entry| entry.path().extension() == Some(OsStr::new("rs")))
            .filter(|entry| {
                if options
                    .ignore_file
                    .iter()
                    .any(|ignored| entry.path().starts_with(ignored))
                {
                    info!("Ignoring file: {}", entry.path().display());
                    false
                } else {
                    true
                }
            })
            .map(|entry| SourceFile {
                path: entry.into_path(),
            })
            .inspect(|file| {
                info!("Collecting file: {}", file.path.display());
            })
            .collect::<Vec<_>>();

        if files.is_empty() {
            bail!("Did not find any files for path {}", path.display());
        }

        if options.rustc && files.len() > 1 {
            bail!("Found more than one file. --rustc only works with a single file.");
        }

        Ok(Self {
            files,
            build,
            options,
            cancel,
        })
    }

    pub(crate) fn run_passes<'a>(
        &self,
        passes: impl IntoIterator<Item = Box<dyn Pass + 'a>>,
    ) -> Result<()> {
        let inital_build = self.build.build()?;
        info!("Initial build: {inital_build}");
        inital_build.require_reproduction("Initial")?;

        for mut pass in passes {
            if self.pass_disabled(pass.name()) {
                continue;
            }
            self.run_pass(&mut *pass)?;
        }

        Ok(())
    }

    fn run_pass(&self, pass: &mut dyn Pass) -> Result<()> {
        let mut invalidated_files = HashSet::new();
        let mut refresh_and_try_again = false;
        loop {
            let span = info_span!("Starting round of pass", name = pass.name());
            let _enter = span.enter();
            let mut changes = Changes::default();

            for file in &self.files {
                if invalidated_files.contains(file) {
                    continue;
                }
                self.process_file(pass, file, &mut invalidated_files, &mut changes)?;
            }

            if !changes.had_changes() {
                if !refresh_and_try_again && !invalidated_files.is_empty() {
                    pass.refresh_state().context("refreshing state for pass")?;
                    invalidated_files.clear();
                    refresh_and_try_again = true;
                    info!("Refreshing files for {}", pass.name());
                    continue;
                }

                info!("Finished {}", pass.name());

                return Ok(());
            } else {
                refresh_and_try_again = false;
            }
        }
    }

    #[instrument(skip(self, pass, invalidated_files, changes), fields(pass = %pass.name()), level = "debug")]
    fn process_file<'file>(
        &self,
        pass: &mut dyn Pass,
        file: &'file SourceFile,
        invalidated_files: &mut HashSet<&'file SourceFile>,
        changes: &mut Changes,
    ) -> Result<()> {
        // The core logic of minimization.
        // Here we process a single file (a unit of work) for a single pass.
        // For this, we repeatedly try to apply a pass to a subset of a file until we've exhausted all options.
        // The logic for bisecting down lives in PassController.

        let mut checker = PassController::new(self.options.clone());
        loop {
            let file_display = file.path.display();
            let mut change = file.try_change(changes)?;
            let mut krate = syn::parse_file(change.before_content())
                .with_context(|| format!("parsing file {file_display}"))?;
            let has_made_change = pass.process_file(&mut krate, file, &mut checker);

            match has_made_change {
                ProcessState::Changed | ProcessState::FileInvalidated => {
                    let result = prettyplease::unparse(&krate);

                    change.write(&result)?;

                    let after = self.build.build()?;
                    info!("{file_display}: After {}: {after}", pass.name());

                    if after.reproduces_issue() {
                        change.commit();
                        checker.reproduces();
                    } else {
                        change.rollback()?;
                        checker.does_not_reproduce();
                    }

                    if has_made_change == ProcessState::FileInvalidated {
                        invalidated_files.insert(file);
                    }
                }
                ProcessState::NoChange => {
                    if self.options.no_color {
                        info!("{file_display}: After {}: no changes", pass.name());
                    } else {
                        info!(
                            "{file_display}: After {}: {}",
                            pass.name(),
                            "no changes".yellow()
                        );
                    }
                    checker.no_change();
                }
            }

            if self.cancel.load(Ordering::SeqCst) {
                info!("Exiting early.");
                std::process::exit(0);
            }

            if checker.is_finished() {
                break;
            }
        }
        Ok(())
    }
}

macro_rules! tracking {
    () => {
        tracking!(visit_item_fn_mut);
        tracking!(visit_impl_item_method_mut);
        tracking!(visit_item_impl_mut);
        tracking!(visit_item_mod_mut);
        tracking!(visit_field_mut);
        tracking!(visit_item_struct_mut);
        tracking!(visit_item_trait_mut);
    };
    (visit_item_fn_mut) => {
        fn visit_item_fn_mut(&mut self, func: &mut syn::ItemFn) {
            self.current_path.push(func.sig.ident.to_string());
            syn::visit_mut::visit_item_fn_mut(self, func);
            self.current_path.pop();
        }
    };
    (visit_impl_item_method_mut) => {
        fn visit_impl_item_method_mut(&mut self, method: &mut syn::ImplItemMethod) {
            self.current_path.push(method.sig.ident.to_string());
            syn::visit_mut::visit_impl_item_method_mut(self, method);
            self.current_path.pop();
        }
    };
    (visit_item_impl_mut) => {
        fn visit_item_impl_mut(&mut self, item: &mut syn::ItemImpl) {
            self.current_path
                .push(item.self_ty.clone().into_token_stream().to_string());
            syn::visit_mut::visit_item_impl_mut(self, item);
            self.current_path.pop();
        }
    };
    (visit_item_mod_mut) => {
        fn visit_item_mod_mut(&mut self, module: &mut syn::ItemMod) {
            self.current_path.push(module.ident.to_string());
            syn::visit_mut::visit_item_mod_mut(self, module);
            self.current_path.pop();
        }
    };
    (visit_field_mut) => {
        fn visit_field_mut(&mut self, field: &mut syn::Field) {
            if let Some(ident) = &field.ident {
                self.current_path.push(ident.to_string());
                syn::visit_mut::visit_field_mut(self, field);
                self.current_path.pop();
            }
        }
    };
    (visit_item_struct_mut) => {
        fn visit_item_struct_mut(&mut self, struct_: &mut syn::ItemStruct) {
            self.current_path.push(struct_.ident.to_string());
            syn::visit_mut::visit_item_struct_mut(self, struct_);
            self.current_path.pop();
        }
    };
    (visit_item_trait_mut) => {
        fn visit_item_trait_mut(&mut self, trait_: &mut syn::ItemTrait) {
            self.current_path.push(trait_.ident.to_string());
            syn::visit_mut::visit_item_trait_mut(self, trait_);
            self.current_path.pop();
        }
    };
}
pub(crate) use tracking;
