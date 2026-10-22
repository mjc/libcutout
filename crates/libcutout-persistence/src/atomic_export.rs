use std::{
    fs::{File, OpenOptions},
    io,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

/// Owns an unpublished export in the destination directory until validation succeeds.
pub(crate) struct AtomicExport {
    temporary: NamedTempFile,
    parent: PathBuf,
    destination: PathBuf,
}

impl AtomicExport {
    pub(crate) fn new(destination: &Path) -> io::Result<Self> {
        let parent = destination
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let temporary = NamedTempFile::new_in(parent)?;
        Ok(Self {
            temporary,
            parent: parent.to_owned(),
            destination: destination.to_owned(),
        })
    }

    pub(crate) fn file_mut(&mut self) -> &mut File {
        self.temporary.as_file_mut()
    }

    pub(crate) fn publish(self) -> io::Result<()> {
        self.temporary.as_file().sync_all()?;
        self.temporary
            .persist_noclobber(&self.destination)
            .map_err(|error| error.error)?;
        OpenOptions::new().read(true).open(&self.parent)?.sync_all()
    }
}
