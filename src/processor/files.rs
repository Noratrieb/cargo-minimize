use anyhow::Result;
use std::{fs, path::Path};

pub(crate) use self::file::SourceFile;

use super::MinimizeEdit;

mod file {
    use anyhow::{Context, Result};
    use std::{
        cell::RefCell,
        path::{Path, PathBuf},
    };

    use crate::processor::MinimizeEdit;

    use super::{Changes, FileChange};

    /// The representation of a source file, with the cached AST.
    /// IMPORTANT INVARIANT: All file system operations MUST go through this type.
    /// This also shouldn't be `Clone`, so the cache is always representative of the file system state.
    /// It is inteded for the "cache" to be the source of truth.
    pub(crate) struct SourceFile {
        path: PathBuf,
        content_str: RefCell<String>,
        content: RefCell<tree_sitter::Tree>,
    }

    impl SourceFile {
        pub(crate) fn open(path: PathBuf) -> Result<Self> {
            let string = std::fs::read_to_string(&path)
                .with_context(|| format!("reading file {}", path.display()))?;

            let content_ts = crate::tree_sitter::parse(&string)
                .with_context(|| format!("parsing file {path:?}"))?;

            Ok(SourceFile {
                path,
                content_str: RefCell::new(string),
                content: RefCell::new(content_ts),
            })
        }

        pub(crate) fn write(&self, new: tree_sitter::Tree, edits: &[MinimizeEdit]) -> Result<()> {
            let string = crate::tree_sitter::apply_edits(new, &*self.content_str.borrow(), edits)?;
            std::fs::write(&self.path, &string)
                .with_context(|| format!("writing file {}", self.path.display()))?;

            let reparsed =
                crate::tree_sitter::parse(&string).expect("failed to reparse after edit");

            *self.content_str.borrow_mut() = string;
            *self.content.borrow_mut() = reparsed;
            Ok(())
        }

        pub(crate) fn path_no_fs_interact(&self) -> &Path {
            &self.path
        }

        pub(crate) fn borrow_tree(&self) -> std::cell::Ref<'_, tree_sitter::Tree> {
            self.content.borrow()
        }

        pub(crate) fn try_change<'file, 'change>(
            &'file self,
            changes: &'change mut Changes,
        ) -> Result<FileChange<'file, 'change>> {
            let path = &self.path;
            Ok(FileChange {
                path,
                source_file: self,
                changes,
                has_written_change: false,
                before_content_str: self.content_str.borrow().clone(),
                before_content: self.content.borrow().clone(),
            })
        }
    }

    impl PartialEq for SourceFile {
        fn eq(&self, other: &Self) -> bool {
            self.path == other.path
        }
    }

    impl std::hash::Hash for SourceFile {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.path.hash(state);
        }
    }

    impl Eq for SourceFile {}

    impl std::fmt::Debug for SourceFile {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.path.display())
        }
    }
}

#[derive(Default)]
pub(crate) struct Changes {
    any_change: bool,
}

pub(crate) struct FileChange<'a, 'b> {
    pub(crate) path: &'a Path,
    source_file: &'a SourceFile,
    before_content_str: String,
    before_content: tree_sitter::Tree,
    changes: &'b mut Changes,
    has_written_change: bool,
}

impl FileChange<'_, '_> {
    pub(crate) fn before_content(&self) -> (&str, &tree_sitter::Tree) {
        (&self.before_content_str, &self.before_content)
    }

    pub(crate) fn write(&mut self, new: tree_sitter::Tree, edits: &[MinimizeEdit]) -> Result<()> {
        self.has_written_change = true;
        self.source_file.write(new, edits)?;
        Ok(())
    }

    pub(crate) fn rollback(mut self) -> Result<()> {
        assert!(self.has_written_change);
        self.has_written_change = false;
        self.source_file.write(self.before_content.clone(), &[])?;
        Ok(())
    }

    pub(crate) fn commit(mut self) {
        assert!(self.has_written_change);
        self.has_written_change = false;
        self.changes.any_change = true;
    }
}

impl Drop for FileChange<'_, '_> {
    fn drop(&mut self) {
        if self.has_written_change {
            fs::write(self.path, self.before_content().0).ok();
            if !std::thread::panicking() {
                panic!("File contains unsaved changes!");
            }
        }
    }
}

impl Changes {
    pub(crate) fn had_changes(&self) -> bool {
        self.any_change
    }
}
