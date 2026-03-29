//! Experimental Asynchronous API for fuser. This is gated behind the "async" feature,
//! and is not yet considered stable. The API may change without a major version bump.

#![allow(unused_variables, unused_mut, clippy::too_many_arguments)]

use std::ffi::OsStr;
use std::path::Path;
use std::time::SystemTime;

use log::{debug, warn};
use tokio::io;

use crate::{
    AccessFlags, BsdFileFlags, Config, CopyFileRangeFlags, Errno, FileHandle, INodeNo,
    KernelConfig, LockOwner, OpenFlags, RenameFlags, Request, TimeOrNow, WriteFlags,
    reply_async::{
        AttrResponse, CreateResponse, DataResponse, DirectoryResponse, EntryResponse,
        GetAttrResponse, LookupResponse, OpenResponse, ReadResponse, StatfsResponse, WriteResponse,
        XattrResponse,
    },
    session_async::AsyncSessionBuilder,
};

/// Async filesystem trait. This is the async version of [`crate::Filesystem`]. It follows a more
/// Rust-idiomatic async API design rather than the C-like, callback-based interface used
/// by [`crate::Filesystem`]. It is not intended to be a thin wrapper over that API.
///
/// Instead of callbacks, this uses a call request -> return result response model, which allows
/// for more straightforward control flow and improved error handling.
///
/// Internally, it operates on an async-aware wrapper ([AsyncFD](https://docs.rs/tokio/latest/tokio/io/unix/struct.AsyncFd.html)) around the FUSE device, enabling
/// better integration and performance with async runtimes.
///
/// For the majority of use cases, users should prefer the [`crate::Filesystem`] API. It is more
/// stable and generally performs better in typical scenarios where the primary bottleneck
/// is the kernel round-trip.
///
/// This API is intended for more IO-bound workloads (e.g. network filesystems), where an
/// async model can improve performance by allowing concurrent request handling and
/// integrating cleanly with other asynchronous systems.
#[async_trait::async_trait]
pub trait AsyncFilesystem: Send + Sync + 'static {
    /// Initialize the filesystem. This is called before the kernel is ready to start sending requests
    /// and to let the filesystem know certain configuration details.
    async fn init(&mut self, _req: &Request, _config: &mut KernelConfig) -> io::Result<()> {
        Ok(())
    }

    /// Clean up filesystem. This is where you drop any resources allocated during the filesystem's
    /// lifetime that won't be automatically cleaned up.
    fn destroy(&mut self) {}

    /// Look up an entry by name and get its attributes. This is called
    /// by the kernel when it needs to know if a file exists and what its attributes are.
    async fn lookup(
        &self,
        context: &Request,
        parent: INodeNo,
        name: &OsStr,
    ) -> Result<LookupResponse, Errno> {
        warn!(
            "lookup not implemented for parent inode {}, name {:?}",
            parent, name
        );
        Err(Errno::ENOTSUP)
    }

    /// Get the attributes of an entry. This is called by the kernel when it needs to know the attributes of
    /// a file, either by inode number or by file handle (created on [`crate::AsyncFilesystem::open`]).
    async fn getattr(
        &self,
        context: &Request,
        ino: INodeNo,
        file_handle: Option<FileHandle>,
    ) -> Result<GetAttrResponse, Errno> {
        warn!("getattr not implemented for inode {}", ino);
        Err(Errno::ENOTSUP)
    }

    /// Forget about an inode. Internal only, the kernel is just making us aware for
    /// bookkeeping.
    async fn forget(&self, req: &Request, ino: INodeNo, nlookup: u64) {
        debug!("forget not implemented for inode {}", ino);
    }

    /// Set file attributes.
    async fn setattr(
        &self,
        req: &Request,
        ino: INodeNo,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<TimeOrNow>,
        mtime: Option<TimeOrNow>,
        ctime: Option<SystemTime>,
        fh: Option<FileHandle>,
        crtime: Option<SystemTime>,
        chgtime: Option<SystemTime>,
        bkuptime: Option<SystemTime>,
        flags: Option<BsdFileFlags>,
    ) -> Result<AttrResponse, Errno> {
        warn!(
            "setattr not implemented for inode {}, mode {:?}, uid {:?}, gid {:?}, size {:?}, fh {:?}, flags {:?}",
            ino, mode, uid, gid, size, fh, flags
        );
        Err(Errno::ENOTSUP)
    }

    /// Read a symbolic link.
    async fn readlink(&self, req: &Request, ino: INodeNo) -> Result<DataResponse, Errno> {
        warn!("readlink not implemented for inode {}", ino);
        Err(Errno::ENOTSUP)
    }

    /// Create file node.
    async fn mknod(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        umask: u32,
        rdev: u32,
    ) -> Result<EntryResponse, Errno> {
        warn!(
            "mknod not implemented for parent inode {}, name {:?}, mode {}, umask {}, rdev {}",
            parent, name, mode, umask, rdev
        );
        Err(Errno::ENOTSUP)
    }

    /// Create a directory.
    async fn mkdir(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        umask: u32,
    ) -> Result<EntryResponse, Errno> {
        warn!(
            "mkdir not implemented for parent inode {}, name {:?}, mode {}, umask {}",
            parent, name, mode, umask
        );
        Err(Errno::ENOTSUP)
    }

    /// Remove a file.
    async fn unlink(&self, req: &Request, parent: INodeNo, name: &OsStr) -> Result<(), Errno> {
        warn!(
            "unlink not implemented for parent inode {}, name {:?}",
            parent, name
        );
        Err(Errno::ENOTSUP)
    }

    /// Remove a directory.
    async fn rmdir(&self, req: &Request, parent: INodeNo, name: &OsStr) -> Result<(), Errno> {
        warn!(
            "rmdir not implemented for parent inode {}, name {:?}",
            parent, name
        );
        Err(Errno::ENOTSUP)
    }

    /// Create a symbolic link.
    async fn symlink(
        &self,
        req: &Request,
        parent: INodeNo,
        link_name: &OsStr,
        target: &Path,
    ) -> Result<EntryResponse, Errno> {
        warn!(
            "symlink not implemented for parent inode {}, link_name {:?}, target {:?}",
            parent, link_name, target
        );
        Err(Errno::ENOTSUP)
    }

    /// Rename a file.
    async fn rename(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        newparent: INodeNo,
        newname: &OsStr,
        flags: RenameFlags,
    ) -> Result<(), Errno> {
        warn!(
            "rename not implemented for parent inode {}, name {:?}, newparent {}, newname {:?}, flags {:?}",
            parent, name, newparent, newname, flags
        );
        Err(Errno::ENOTSUP)
    }

    /// Create a hard link.
    async fn link(
        &self,
        req: &Request,
        ino: INodeNo,
        newparent: INodeNo,
        newname: &OsStr,
    ) -> Result<EntryResponse, Errno> {
        warn!(
            "link not implemented for inode {}, newparent {}, newname {:?}",
            ino, newparent, newname
        );
        Err(Errno::ENOTSUP)
    }

    /// Open a file.
    async fn open(
        &self,
        req: &Request,
        ino: INodeNo,
        flags: OpenFlags,
    ) -> Result<OpenResponse, Errno> {
        warn!("open not implemented for inode {}, flags {:?}", ino, flags);
        Err(Errno::ENOTSUP)
    }

    /// Return the data of a file. This is called by the kernel when it needs to read the contents
    /// of a file.
    async fn read(
        &self,
        req: &Request,
        ino: INodeNo,
        file_handle: FileHandle,
        offset: u64,
        size: u32,
        flags: OpenFlags,
        lock: Option<LockOwner>,
    ) -> Result<ReadResponse, Errno> {
        warn!(
            "read not implemented for inode {}, offset {}, size {}",
            ino, offset, size
        );
        Err(Errno::ENOTSUP)
    }

    /// Write data.
    ///
    /// Write should return exactly the number of bytes requested except on error. An
    /// exception to this is when the file has been opened in `direct_io` mode, in
    /// which case the return value of the write system call will reflect the return
    /// value of this operation. fh will contain the value set by the open method, or
    /// will be undefined if the open method didn't set any value.
    ///
    /// `write_flags`: will contain `FUSE_WRITE_CACHE`, if this write is from the page cache. If set,
    /// the pid, uid, gid, and fh may not match the value that would have been sent if write cachin
    /// is disabled
    /// flags: these are the file flags, such as `O_SYNC`. Only supported with ABI >= 7.9
    /// `lock_owner`: only supported with ABI >= 7.9
    async fn write(
        &self,
        req: &Request,
        ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        data: &[u8],
        write_flags: WriteFlags,
        flags: OpenFlags,
        lock_owner: Option<LockOwner>,
    ) -> Result<WriteResponse, Errno> {
        warn!(
            "write not implemented for inode {}, offset {}, size {}",
            ino,
            offset,
            data.len()
        );
        Err(Errno::ENOTSUP)
    }

    /// Release an open file.
    async fn release(
        &self,
        req: &Request,
        ino: INodeNo,
        fh: FileHandle,
        flags: OpenFlags,
        lock_owner: Option<LockOwner>,
        flush: bool,
    ) -> Result<(), Errno> {
        warn!("release not implemented for inode {}, fh {:?}", ino, fh);
        Err(Errno::ENOTSUP)
    }

    /// Open a directory.
    async fn opendir(
        &self,
        req: &Request,
        ino: INodeNo,
        flags: OpenFlags,
    ) -> Result<OpenResponse, Errno> {
        warn!(
            "opendir not implemented for inode {}, flags {:?}",
            ino, flags
        );
        Err(Errno::ENOTSUP)
    }

    /// Construct a directory listing response for the given directory inode. This is called by
    /// the kernel when it needs to read the contents of a directory.
    async fn readdir(
        &self,
        req: &Request,
        ino: INodeNo,
        file_handle: FileHandle,
        size: u32,
        offset: u64,
    ) -> Result<DirectoryResponse, Errno> {
        warn!(
            "readdir not implemented for inode {}, offset {}, size {}",
            ino, offset, size
        );
        Err(Errno::ENOTSUP)
    }

    /// Release an open directory.
    async fn releasedir(
        &self,
        req: &Request,
        ino: INodeNo,
        fh: FileHandle,
        flags: OpenFlags,
    ) -> Result<(), Errno> {
        warn!("releasedir not implemented for inode {}, fh {:?}", ino, fh);
        Err(Errno::ENOTSUP)
    }

    /// Get file system statistics.
    async fn statfs(&self, req: &Request, ino: INodeNo) -> Result<StatfsResponse, Errno> {
        warn!("statfs not implemented for inode {}", ino);
        Err(Errno::ENOTSUP)
    }

    /// Set an extended attribute.
    async fn setxattr(
        &self,
        req: &Request,
        ino: INodeNo,
        name: &OsStr,
        value: &[u8],
        flags: i32,
        position: u32,
    ) -> Result<(), Errno> {
        warn!(
            "setxattr not implemented for inode {}, name {:?}, flags {}, position {}",
            ino, name, flags, position
        );
        Err(Errno::ENOTSUP)
    }

    /// Get an extended attribute.
    async fn getxattr(
        &self,
        req: &Request,
        ino: INodeNo,
        name: &OsStr,
        size: u32,
    ) -> Result<XattrResponse, Errno> {
        warn!(
            "getxattr not implemented for inode {}, name {:?}, size {}",
            ino, name, size
        );
        Err(Errno::ENOTSUP)
    }

    /// List extended attribute names.
    async fn listxattr(
        &self,
        req: &Request,
        ino: INodeNo,
        size: u32,
    ) -> Result<XattrResponse, Errno> {
        warn!("listxattr not implemented for inode {}, size {}", ino, size);
        Err(Errno::ENOTSUP)
    }

    /// Remove an extended attribute.
    async fn removexattr(&self, req: &Request, ino: INodeNo, name: &OsStr) -> Result<(), Errno> {
        warn!(
            "removexattr not implemented for inode {}, name {:?}",
            ino, name
        );
        Err(Errno::ENOTSUP)
    }

    /// Check file access permissions.
    async fn access(&self, req: &Request, ino: INodeNo, mask: AccessFlags) -> Result<(), Errno> {
        warn!("access not implemented for inode {}, mask {:?}", ino, mask);
        Err(Errno::ENOTSUP)
    }

    /// Create and open a file.
    async fn create(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        umask: u32,
        flags: i32,
    ) -> Result<CreateResponse, Errno> {
        warn!(
            "create not implemented for parent inode {}, name {:?}, mode {}, umask {}, flags {}",
            parent, name, mode, umask, flags
        );
        Err(Errno::ENOTSUP)
    }

    /// Preallocate or deallocate file space.
    async fn fallocate(
        &self,
        req: &Request,
        ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        length: u64,
        mode: i32,
    ) -> Result<(), Errno> {
        warn!(
            "fallocate not implemented for inode {}, offset {}, length {}, mode {}",
            ino, offset, length, mode
        );
        Err(Errno::ENOTSUP)
    }

    /// Copy a file range.
    async fn copy_file_range(
        &self,
        req: &Request,
        ino_in: INodeNo,
        fh_in: FileHandle,
        offset_in: u64,
        ino_out: INodeNo,
        fh_out: FileHandle,
        offset_out: u64,
        len: u64,
        flags: CopyFileRangeFlags,
    ) -> Result<WriteResponse, Errno> {
        warn!(
            "copy_file_range not implemented for src ({}, {}, {}), dest ({}, {}, {}), len {}, flags {:?}",
            ino_in, fh_in, offset_in, ino_out, fh_out, offset_out, len, flags
        );
        Err(Errno::ENOTSUP)
    }
}

/// Mount the given async filesystem to the given mountpoint. This function will
/// not return until the filesystem is unmounted.
///
/// # Errors
/// Returns an error if the options are incorrect, or if the fuse device can't be mounted,
/// and any final error when the session comes to an end.
pub async fn mount_async<FS: AsyncFilesystem, P: AsRef<Path>>(
    filesystem: FS,
    mountpoint: P,
    options: &Config,
) -> io::Result<()> {
    let session = AsyncSessionBuilder::new()
        .filesystem(filesystem)
        .mountpoint(mountpoint)
        .options(options.clone())?
        .build()
        .await?;
    session.run().await
}
