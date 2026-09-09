//! Virtual File System (VFS) Layer.
//!
//! Exposes unified POSIX-like file and directory operations backed by
//! the mounted root filesystem (`TarFs`).

use super::tarfs::TarFs;
use crate::sync::SpinLock;
use alloc::string::String;
use alloc::vec::Vec;

/// Filesystem errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    NotFound,
    NotAFile,
    NotADirectory,
    IoError,
    AlreadyOpen,
    InvalidPath,
    NoFileSystemMounted,
}

/// Metadata describing a file or directory node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    pub path: String,
    pub size: usize,
    pub is_dir: bool,
}

/// Directory entry description for listing operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: usize,
}

/// Active handle for streaming file reads.
pub struct FileHandle {
    pub path: String,
    pub size: usize,
    pub cursor: usize,
    data: Vec<u8>,
}

impl FileHandle {
    pub fn new(path: String, data: Vec<u8>) -> Self {
        let size = data.len();
        FileHandle {
            path,
            size,
            cursor: 0,
            data,
        }
    }

    /// Reads up to `buf.len()` bytes into `buf`, advancing the seek cursor.
    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        if self.cursor >= self.size {
            return 0;
        }

        let remaining = self.size - self.cursor;
        let to_copy = remaining.min(buf.len());
        buf[..to_copy].copy_from_slice(&self.data[self.cursor..self.cursor + to_copy]);
        self.cursor += to_copy;
        to_copy
    }

    /// Resets the seek cursor back to start of file.
    pub fn rewind(&mut self) {
        self.cursor = 0;
    }
}

/// Global system VFS instance holding the mounted root filesystem.
pub static VFS: SpinLock<Option<TarFs>> = SpinLock::new(None);

/// Mounts the root filesystem into the VFS.
pub fn mount_root(fs: TarFs) {
    *VFS.lock() = Some(fs);
}

/// Returns true if a root filesystem is currently mounted.
pub fn is_mounted() -> bool {
    VFS.lock().is_some()
}

/// Opens an existing file for reading, returning a seekable `FileHandle`.
pub fn open(path: &str) -> Result<FileHandle, FsError> {
    let guard = VFS.lock();
    let fs = guard.as_ref().ok_or(FsError::NoFileSystemMounted)?;

    let entry = fs.find_entry(path).ok_or(FsError::NotFound)?;
    if entry.is_dir {
        return Err(FsError::NotAFile);
    }

    let data = fs.read_file(entry)?;
    Ok(FileHandle::new(entry.path.clone(), data))
}

/// Reads from an open `FileHandle`.
pub fn read(handle: &mut FileHandle, buf: &mut [u8]) -> Result<usize, FsError> {
    Ok(handle.read(buf))
}

/// Reads the entire contents of a file into a byte buffer.
pub fn read_all(path: &str) -> Result<Vec<u8>, FsError> {
    let guard = VFS.lock();
    let fs = guard.as_ref().ok_or(FsError::NoFileSystemMounted)?;

    let entry = fs.find_entry(path).ok_or(FsError::NotFound)?;
    fs.read_file(entry)
}

/// Reads the entire contents of a file as a UTF-8 string.
pub fn read_to_string(path: &str) -> Result<String, FsError> {
    let bytes = read_all(path)?;
    String::from_utf8(bytes).map_err(|_| FsError::IoError)
}

/// Queries metadata for the specified file or directory path.
pub fn stat(path: &str) -> Result<FileInfo, FsError> {
    let guard = VFS.lock();
    let fs = guard.as_ref().ok_or(FsError::NoFileSystemMounted)?;
    fs.stat(path)
}

/// Lists directory contents for the specified path.
pub fn list_dir(path: &str) -> Result<Vec<DirEntry>, FsError> {
    let guard = VFS.lock();
    let fs = guard.as_ref().ok_or(FsError::NoFileSystemMounted)?;
    fs.list_dir(path)
}
