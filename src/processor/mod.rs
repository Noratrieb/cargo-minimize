mod files;
mod reaper;
pub(crate) use self::files::SourceFile;
use self::worklist::Worklist;
use crate::{build::Build, processor::files::Changes, Options};
use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use std::{
    borrow::Borrow,
    collections::{BTreeSet, HashSet},
    ffi::OsStr,
    fmt::Debug,
    mem,
};

pub(crate) trait Processor {
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
}

impl Debug for dyn Processor {
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
}

impl Minimizer {
    pub(crate) fn new_glob_dir(options: Options, build: Build) -> Self {
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
            .map(|entry| SourceFile {
                path: entry.into_path(),
            })
            .inspect(|file| {
                info!("Collecting file: {}", file.path.display());
            })
            .collect();

        Self {
            files,
            build,
            options,
        }
    }

    pub(crate) fn run_passes<'a>(
        &self,
        passes: impl IntoIterator<Item = Box<dyn Processor + 'a>>,
    ) -> Result<()> {
        let inital_build = self.build.build()?;
        info!("Initial build: {inital_build}");
        inital_build.require_reproduction("Initial")?;

        for mut pass in passes {
            self.run_pass(&mut *pass)?;
        }

        Ok(())
    }

    fn run_pass(&self, pass: &mut dyn Processor) -> Result<()> {
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

    fn process_file<'file>(
        &self,
        pass: &mut dyn Processor,
        file: &'file SourceFile,
        invalidated_files: &mut HashSet<&'file SourceFile>,
        changes: &mut Changes,
    ) -> Result<()> {
        let mut checker = PassController::new();
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

            if checker.is_finished() {
                break;
            }
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct AstPath(Vec<String>);

impl Borrow<[String]> for AstPath {
    fn borrow(&self) -> &[String] {
        &self.0
    }
}

impl Debug for AstPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AstPath({:?})", self.0)
    }
}

#[derive(Debug)]
pub(crate) struct PassController {
    state: PassControllerState,
}

#[derive(Debug)]
enum PassControllerState {
    InitialCollection {
        candidates: Vec<AstPath>,
    },
    Bisecting {
        committed: BTreeSet<AstPath>,
        failed: BTreeSet<AstPath>,
        current: BTreeSet<AstPath>,
        worklist: Worklist,
    },
    Success,
}

mod worklist {
    use super::AstPath;

    /// A worklist that ensures that the inner list is never empty.
    #[derive(Debug)]
    pub(super) struct Worklist(Vec<Vec<AstPath>>);

    impl Worklist {
        pub(super) fn new() -> Self {
            Self(Vec::new())
        }

        pub(super) fn push(&mut self, next: Vec<AstPath>) {
            if !next.is_empty() {
                self.0.push(next);
            }
        }

        pub(super) fn pop(&mut self) -> Option<Vec<AstPath>> {
            self.0.pop()
        }
    }
}

// copied from `core` because who needs stable features anyways
pub const fn div_ceil(lhs: usize, rhs: usize) -> usize {
    let d = lhs / rhs;
    let r = lhs % rhs;
    if r > 0 && rhs > 0 {
        d + 1
    } else {
        d
    }
}

fn split_owned<T, From: IntoIterator<Item = T>, A: FromIterator<T>, B: FromIterator<T>>(
    vec: From,
) -> (A, B) {
    let candidates = vec.into_iter().collect::<Vec<_>>();
    let half = div_ceil(candidates.len(), 2);

    let mut candidates = candidates.into_iter();

    let first_half = candidates.by_ref().take(half).collect();
    let second_half = candidates.collect();

    (first_half, second_half)
}

impl PassController {
    fn new() -> Self {
        Self {
            state: PassControllerState::InitialCollection {
                candidates: Vec::new(),
            },
        }
    }

    fn next_in_worklist(&mut self) {
        match &mut self.state {
            PassControllerState::Bisecting {
                current, worklist, ..
            } => match worklist.pop() {
                Some(next) => {
                    *current = next.into_iter().collect();
                }
                None => {
                    self.state = PassControllerState::Success;
                }
            },
            _ => unreachable!("next_in_worklist called on non-bisecting state"),
        }
    }

    fn reproduces(&mut self) {
        match &mut self.state {
            PassControllerState::InitialCollection { .. } => {
                self.state = PassControllerState::Success;
            }
            PassControllerState::Bisecting {
                committed,
                failed: _,
                current,
                worklist: _,
            } => {
                committed.extend(mem::take(current));

                self.next_in_worklist();
            }
            PassControllerState::Success => unreachable!("Processed after success"),
        }
    }

    fn does_not_reproduce(&mut self) {
        match &mut self.state {
            PassControllerState::InitialCollection { candidates } => {
                let (current, first_worklist_item) = split_owned(mem::take(candidates));

                let mut worklist = Worklist::new();
                worklist.push(first_worklist_item);

                self.state = PassControllerState::Bisecting {
                    committed: BTreeSet::new(),
                    failed: BTreeSet::new(),
                    current,
                    worklist,
                };
            }
            PassControllerState::Bisecting {
                committed: _,
                failed,
                current,
                worklist,
            } => {
                if current.len() == 1 {
                    // We are at a leaf. This is a failure.
                    // FIXME: We should retry the failed ones until a fixpoint is reached.
                    failed.extend(mem::take(current));
                } else {
                    // Split it further and add it to the worklist.
                    let (first_half, second_half) = split_owned(mem::take(current));

                    worklist.push(first_half);
                    worklist.push(second_half);
                }

                self.next_in_worklist()
            }
            PassControllerState::Success => unreachable!("Processed after success"),
        }
    }

    fn no_change(&mut self) {
        match &self.state {
            PassControllerState::InitialCollection { candidates } => {
                assert!(
                    candidates.is_empty(),
                    "No change but received candidates: {candidates:?}"
                );
                self.state = PassControllerState::Success;
            }
            PassControllerState::Bisecting { current, .. } => {
                unreachable!("No change while bisecting, current was empty somehow: {current:?}");
            }
            PassControllerState::Success => {}
        }
    }

    fn is_finished(&mut self) -> bool {
        match &mut self.state {
            PassControllerState::InitialCollection { .. } => false,
            PassControllerState::Bisecting { .. } => false,
            PassControllerState::Success => true,
        }
    }

    pub(crate) fn can_process(&mut self, path: &[String]) -> bool {
        match &mut self.state {
            PassControllerState::InitialCollection { candidates } => {
                candidates.push(AstPath(path.to_owned()));
                true
            }
            PassControllerState::Bisecting { current, .. } => current.contains(path),
            PassControllerState::Success => {
                unreachable!("Processed further after success");
            }
        }
    }
}

macro_rules! tracking {
    () => {
        tracking!(visit_item_fn_mut);
        tracking!(visit_impl_item_method_mut);
        tracking!(visit_item_impl_mut);
        tracking!(visit_item_mod_mut);
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
}
pub(crate) use tracking;
