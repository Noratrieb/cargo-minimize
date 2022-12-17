use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use anyhow::{ensure, Context, Result};

use crate::build::Build;

pub trait Processor {
    fn process_file(&mut self, krate: &mut syn::File) -> bool;

    fn name(&self) -> &'static str;
}

#[derive(Debug)]
pub struct Minimizer {
    files: Vec<PathBuf>,
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
            .map(|entry| entry.into_path())
            .collect();

        Self { files, build }
    }

    pub fn run_passes<'a>(
        &mut self,
        passes: impl IntoIterator<Item = Box<dyn Processor>>,
    ) -> Result<()> {
        let inital_build = self.build.build()?;
        println!("Initial build: {}", inital_build);
        ensure!(
            inital_build.reproduces_issue,
            "Initial build must reproduce issue"
        );

        for mut pass in passes {
            'pass: loop {
                println!("Starting a round of {}", pass.name());
                let mut any_change = false;

                for file in &self.files {
                    let file_display = file.display();

                    let before_string = std::fs::read_to_string(file)
                        .with_context(|| format!("opening file {file_display}"))?;

                    let mut krate = syn::parse_file(&before_string)
                        .with_context(|| format!("parsing file {file_display}"))?;

                    let has_made_change = pass.process_file(&mut krate);

                    if has_made_change {
                        let result = prettyplease::unparse(&krate);

                        std::fs::write(file, &result)?;

                        let after = self.build.build()?;

                        println!("{file_display}: After {}: {after}", pass.name());

                        if after.reproduces_issue {
                            any_change = true;
                        } else {
                            std::fs::write(file, before_string)?;
                        }
                    } else {
                        println!("{file_display}: After {}: no change", pass.name());
                    }
                }

                if !any_change {
                    println!("Finished {}", pass.name());
                    break 'pass;
                }
            }
        }

        Ok(())
    }
}
