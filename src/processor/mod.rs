mod files;
mod reaper;

use std::{collections::HashSet, ffi::OsStr, path::Path};

use anyhow::{ensure, Context, Result};

use crate::{build::Build, processor::files::Changes};

pub use self::files::SourceFile;

pub trait Processor {
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
        checker: &mut ProcessChecker,
    ) -> ProcessState;

    fn name(&self) -> &'static str;
}

#[derive(Debug, PartialEq, Eq)]
pub enum ProcessState {
    NoChange,
    Changed,
    FileInvalidated,
}

#[derive(Debug)]
pub struct Minimizer {
    files: Vec<SourceFile>,
    build: Build,
}

impl Minimizer {
    pub fn new_glob_dir(path: &Path, build: Build) -> Self {
        let walk = walkdir::WalkDir::new(path);

        let files = walk
            .into_iter()
            .filter_map(|entry| match entry {
                Ok(entry) => Some(entry),
                Err(err) => {
                    eprintln!("WARN: Error in walkdir: {err}");
                    None
                }
            })
            .filter(|entry| entry.path().extension() == Some(OsStr::new("rs")))
            .map(|entry| SourceFile {
                path: entry.into_path(),
            })
            .inspect(|file| {
                println!("- {}", file.path.display());
            })
            .collect();

        Self { files, build }
    }

    pub fn run_passes<'a>(
        &mut self,
        passes: impl IntoIterator<Item = Box<dyn Processor + 'a>>,
    ) -> Result<()> {
        let inital_build = self.build.build()?;
        println!("Initial build: {}", inital_build);
        ensure!(
            inital_build.reproduces_issue(),
            "Initial build must reproduce issue"
        );

        for mut pass in passes {
            self.run_pass(&mut *pass)?;
        }

        Ok(())
    }

    fn run_pass(&mut self, pass: &mut dyn Processor) -> Result<()> {
        let mut invalidated_files = HashSet::new();

        let mut refresh_and_try_again = false;

        'pass: loop {
            println!("Starting a round of {}", pass.name());
            let mut changes = Changes::default();

            for file in &self.files {
                if invalidated_files.contains(file) {
                    continue;
                }

                let file_display = file.path.display();

                let mut change = file.try_change(&mut changes)?;

                let mut krate = syn::parse_file(change.before_content())
                    .with_context(|| format!("parsing file {file_display}"))?;

                let has_made_change = pass.process_file(&mut krate, file, &mut ProcessChecker {});

                match has_made_change {
                    ProcessState::Changed | ProcessState::FileInvalidated => {
                        let result = prettyplease::unparse(&krate);

                        change.write(&result)?;

                        let after = self.build.build()?;

                        println!("{file_display}: After {}: {after}", pass.name());

                        if after.reproduces_issue() {
                            change.commit();
                        } else {
                            change.rollback()?;
                        }

                        if has_made_change == ProcessState::FileInvalidated {
                            invalidated_files.insert(file);
                        }
                    }
                    ProcessState::NoChange => {
                        println!("{file_display}: After {}: no change", pass.name());
                    }
                }
            }

            if !changes.had_changes() {
                if !refresh_and_try_again && invalidated_files.len() > 0 {
                    // A few files have been invalidated, let's refresh and try these again.
                    pass.refresh_state().context("refreshing state for pass")?;
                    invalidated_files.clear();
                    refresh_and_try_again = true;
                    println!("Refreshing files for {}", pass.name());
                    continue;
                }

                println!("Finished {}", pass.name());
                break 'pass;
            } else {
                refresh_and_try_again = false;
            }
        }

        Ok(())
    }
}

pub struct ProcessChecker {}

impl ProcessChecker {
    pub fn can_process(&mut self, _: &[String]) -> bool {
        true
    }
}
