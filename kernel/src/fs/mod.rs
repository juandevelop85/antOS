//! File system and VFS subsystem for antOS.

pub mod tarfs;
pub mod vfs;

pub use tarfs::{TarBacking, TarEntry, TarFs};
pub use vfs::{DirEntry, FileHandle, FileInfo, FsError};
