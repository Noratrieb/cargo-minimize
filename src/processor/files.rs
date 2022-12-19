use anyhow::{Context, Result};
use std::{fs, path::{Path, PathBuf}};
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub(crate) struct SourceFile {
    pub(crate) path: PathBuf,
}
#[derive(Default)]
pub(crate) struct Changes {
    any_change: bool,
}
pub(crate) struct FileChange<'a, 'b> {
    pub(crate) path: &'a Path,
    content: String,
    changes: &'b mut Changes,
    has_written_change: bool,
}
impl FileChange<'_, '_> {
    pub(crate) fn before_content(&self) -> &str {
        &self.content
    }
    pub(crate) fn write(&mut self, new: &str) -> Result<()> {
        self.has_written_change = true;
        fs::write(self.path, new)
            .with_context(|| format!("writing file {}", self.path.display()))
    }
    pub(crate) fn rollback(mut self) -> Result<()> {
        assert!(self.has_written_change);
        self.has_written_change = false;
        fs::write(self.path, &self.content)
            .with_context(|| format!("writing file {}", self.path.display()))
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
            fs::write(self.path, self.before_content()).ok();
            if !std::thread::panicking() {
                panic!("File contains unsaved changes!");
            }
        }
    }
}
impl SourceFile {
    pub(crate) fn try_change<'file, 'change>(
        &'file self,
        changes: &'change mut Changes,
    ) -> Result<FileChange<'file, 'change>> {
        let path = &self.path;
        Ok(FileChange {
            path,
            changes,
            has_written_change: false,
            content: fs::read_to_string(path)
                .with_context(|| format!("opening file {}", path.display()))?,
        })
    }
}
impl Changes {
    pub(crate) fn had_changes(&self) -> bool {
        self.any_change
    }
}
