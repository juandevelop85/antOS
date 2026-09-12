//! Virtual File System (VFS) Layer.
//!
//! Exposes unified POSIX-like file and directory operations backed by
//! the mounted root filesystem (`TarFs`).

use super::tarfs::TarFs;
use crate::sync::SpinLock;
use alloc::borrow::Cow;
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
    data: Cow<'static, [u8]>,
}

impl FileHandle {
    pub fn new(path: String, data: Vec<u8>) -> Self {
        let size = data.len();
        FileHandle {
            path,
            size,
            cursor: 0,
            data: Cow::Owned(data),
        }
    }

    /// Creates a handle borrowing static memory directly (zero-copy).
    pub fn from_borrowed(path: String, data: &'static [u8]) -> Self {
        let size = data.len();
        FileHandle {
            path,
            size,
            cursor: 0,
            data: Cow::Borrowed(data),
        }
    }

    /// Creates a handle from a `Cow<'static, [u8]>`.
    pub fn from_cow(path: String, data: Cow<'static, [u8]>) -> Self {
        let size = data.len();
        FileHandle {
            path,
            size,
            cursor: 0,
            data,
        }
    }

    /// Returns true if the underlying file buffer is zero-copy borrowed memory.
    pub fn is_borrowed(&self) -> bool {
        matches!(self.data, Cow::Borrowed(_))
    }

    /// Accesses the underlying byte slice directly without heap copying.
    pub fn data(&self) -> &[u8] {
        &self.data
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
/// When backed by in-memory storage (`TarBacking::Memory`), the file data is zero-copy.
pub fn open(path: &str) -> Result<FileHandle, FsError> {
    let guard = VFS.lock();
    let fs = guard.as_ref().ok_or(FsError::NoFileSystemMounted)?;

    let entry = fs.find_entry(path).ok_or(FsError::NotFound)?;
    if entry.is_dir {
        return Err(FsError::NotAFile);
    }

    let data = fs.read_file_cow(entry)?;
    Ok(FileHandle::from_cow(entry.path.clone(), data))
}

/// Reads from an open `FileHandle`.
pub fn read(handle: &mut FileHandle, buf: &mut [u8]) -> Result<usize, FsError> {
    Ok(handle.read(buf))
}

/// Reads the entire contents of a file as a `Cow<'static, [u8]>`.
/// When backed by in-memory storage, this operation does not allocate heap memory for the content.
pub fn read_all_cow(path: &str) -> Result<Cow<'static, [u8]>, FsError> {
    let guard = VFS.lock();
    let fs = guard.as_ref().ok_or(FsError::NoFileSystemMounted)?;

    let entry = fs.find_entry(path).ok_or(FsError::NotFound)?;
    fs.read_file_cow(entry)
}

/// Reads the entire contents of a file into a byte buffer.
pub fn read_all(path: &str) -> Result<Vec<u8>, FsError> {
    read_all_cow(path).map(Cow::into_owned)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_file_handle_borrowed_and_owned() {
        static BORROWED: [u8; 8] = *b"borrowed";
        let mut handle = FileHandle::from_borrowed("/mem.txt".into(), &BORROWED);
        assert!(handle.is_borrowed());
        assert_eq!(handle.size, 8);
        assert_eq!(handle.data(), b"borrowed");

        let mut out = [0u8; 4];
        assert_eq!(handle.read(&mut out), 4);
        assert_eq!(&out, b"borr");
        assert_eq!(handle.read(&mut out), 4);
        assert_eq!(&out, b"owed");
        assert_eq!(handle.read(&mut out), 0);

        handle.rewind();
        assert_eq!(handle.read(&mut out), 4);
        assert_eq!(&out, b"borr");

        let owned_handle = FileHandle::new("/owned.txt".into(), alloc::vec![1, 2, 3]);
        assert!(!owned_handle.is_borrowed());
        assert_eq!(owned_handle.data(), &[1, 2, 3]);
    }

    #[test_case]
    fn test_vfs_open_zero_copy_borrowed() {
        // Build a minimal in-memory USTAR archive: 512-byte header + 512-byte data block + 1024 zero bytes
        static MOCK_TAR: [u8; 2048] = {
            let mut buf = [0u8; 2048];
            // Name "hello.txt"
            buf[0] = b'h'; buf[1] = b'e'; buf[2] = b'l'; buf[3] = b'l'; buf[4] = b'o';
            buf[5] = b'.'; buf[6] = b't'; buf[7] = b'x'; buf[8] = b't';
            // Mode "0000644\0"
            buf[100] = b'0'; buf[101] = b'0'; buf[102] = b'0'; buf[103] = b'0';
            buf[104] = b'6'; buf[105] = b'4'; buf[106] = b'4';
            // Size "00000000013\0" (octal 13 = 11 decimal bytes)
            buf[124] = b'0'; buf[125] = b'0'; buf[126] = b'0'; buf[127] = b'0';
            buf[128] = b'0'; buf[129] = b'0'; buf[130] = b'0'; buf[131] = b'0';
            buf[132] = b'0'; buf[133] = b'1'; buf[134] = b'3';
            // Magic "ustar\0"
            buf[257] = b'u'; buf[258] = b's'; buf[259] = b't'; buf[260] = b'a'; buf[261] = b'r';
            // Version "00"
            buf[263] = b'0'; buf[264] = b'0';
            // Content "hello antos" at 512
            buf[512] = b'h'; buf[513] = b'e'; buf[514] = b'l'; buf[515] = b'l'; buf[516] = b'o';
            buf[517] = b' ';
            buf[518] = b'a'; buf[519] = b'n'; buf[520] = b't'; buf[521] = b'o'; buf[522] = b's';
            buf
        };

        let fs = TarFs::from_memory(&MOCK_TAR).expect("parse mock tar");
        mount_root(fs);
        assert!(is_mounted());

        let mut handle = open("/hello.txt").expect("open hello.txt");
        assert!(handle.is_borrowed(), "vfs::open on TarBacking::Memory must be zero-copy borrowed");
        assert_eq!(handle.size, 11);

        let mut buf = [0u8; 11];
        let n = handle.read(&mut buf);
        assert_eq!(n, 11);
        assert_eq!(&buf, b"hello antos");

        let cow_data = read_all_cow("/hello.txt").expect("read_all_cow");
        assert!(matches!(cow_data, Cow::Borrowed(_)));
        assert_eq!(&*cow_data, b"hello antos");

        let str_data = read_to_string("/hello.txt").expect("read_to_string");
        assert_eq!(str_data, "hello antos");
    }
}
