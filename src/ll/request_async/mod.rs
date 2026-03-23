//! Filesystem operation request
//!
//! A request represents information about a filesystem operation the kernel driver wants us to
//! perform.

use std::convert::TryFrom;
use std::ffi::OsString;

use log::debug;
use log::error;

use crate::AsyncFilesystem;
use crate::FileHandle;
use crate::INodeNo;
use crate::LockOwner;
use crate::OpenFlags;
use crate::Request;
use crate::channel_async::AsyncChannelSender;
use crate::ll;
use crate::ll::Errno;
use crate::ll::ResponseErrno;
use crate::ll::fuse_abi::fuse_in_header;
use crate::ll::reply::Response;
use crate::session::SessionACL;
use crate::session_async::AsyncSessionEventLoop;

#[derive(Debug)]
enum AsyncOperation {
    Lookup {
        parent: INodeNo,
        name: OsString,
    },
    ReadDir {
        ino: INodeNo,
        file_handle: FileHandle,
        size: u32,
        offset: u64,
    },
    GetAttr {
        ino: INodeNo,
        file_handle: Option<FileHandle>,
    },
    Read {
        ino: INodeNo,
        file_handle: FileHandle,
        offset: u64,
        size: u32,
        flags: OpenFlags,
        lock_owner: Option<LockOwner>,
    },
    Init,
    Destroy,
    Unsupported,
}

/// Asynchronous request data structure (request from the kernel along with a
/// channel sender clone for sending the reply).
#[derive(Debug)]
pub(crate) struct AsyncRequestWithSender {
    /// Async sender for sending the reply
    ch: AsyncChannelSender,
    /// Request header copied out of the kernel buffer so the request can be moved to a task.
    header: fuse_in_header,
    /// Owned operation data used by the async dispatcher.
    operation: AsyncOperation,
}

impl AsyncRequestWithSender {
    /// Create a new request from the given data.
    pub(crate) fn new(
        ch: AsyncChannelSender,
        data: &[u8],
    ) -> Result<AsyncRequestWithSender, tokio::io::Error> {
        let request = ll::AnyRequest::try_from(data).map_err(|err| {
            error!("Failed to parse request from kernel: {}", err);
            tokio::io::Error::new(tokio::io::ErrorKind::InvalidData, "Failed to parse request")
        })?;
        let operation = match request.operation() {
            Ok(ll::Operation::Lookup(x)) => AsyncOperation::Lookup {
                parent: request.nodeid(),
                name: x.name().as_os_str().to_os_string(),
            },
            Ok(ll::Operation::ReadDir(x)) => AsyncOperation::ReadDir {
                ino: request.nodeid(),
                file_handle: x.file_handle(),
                size: x.size(),
                offset: x.offset(),
            },
            Ok(ll::Operation::GetAttr(x)) => AsyncOperation::GetAttr {
                ino: request.nodeid(),
                file_handle: x.file_handle(),
            },
            Ok(ll::Operation::Read(x)) => AsyncOperation::Read {
                ino: request.nodeid(),
                file_handle: x.file_handle(),
                offset: x.offset().map_err(|e| {
                    tokio::io::Error::new(
                        tokio::io::ErrorKind::InvalidData,
                        format!("Invalid read offset: {e:?}"),
                    )
                })?,
                size: x.size(),
                flags: x.flags(),
                lock_owner: x.lock_owner(),
            },
            Ok(ll::Operation::Init(_)) => AsyncOperation::Init,
            Ok(ll::Operation::Destroy(_)) => AsyncOperation::Destroy,
            Ok(_) => AsyncOperation::Unsupported,
            Err(_) => AsyncOperation::Unsupported,
        };

        Ok(Self {
            ch,
            // SAFETY: `fuse_in_header` is a plain FUSE ABI POD struct with no drop glue.
            header: unsafe { std::ptr::read(request.header()) },
            operation,
        })
    }

    /// Async dispatch request to the given filesystem, takes the return from the filesystem and
    /// sends the reply back to the kernel. This follows a more Rust-idiomatic async API design rather than the C-like,
    /// callback-based interface used in [`crate::Filesystem`].
    pub(crate) async fn dispatch<FS: AsyncFilesystem>(&self, session: &AsyncSessionEventLoop<FS>) {
        debug!(
            "FUSE({}) ino {:#018x} thread={}",
            self.header.unique, self.header.nodeid, session.thread_name
        );

        if let Err(errno) = self.dispatch_req(session).await {
            let response = ResponseErrno(errno);
            self.reply(&response).await.unwrap_or_else(|e| {
                error!(
                    "Failed to send error reply for request {}: {e:?}",
                    self.header.unique
                );
            });
        }
    }

    async fn dispatch_req<FS: AsyncFilesystem>(
        &self,
        session: &AsyncSessionEventLoop<FS>,
    ) -> Result<(), Errno> {
        let req_uid = nix::unistd::Uid::from_raw(self.request_header().uid());
        if (session.allowed == SessionACL::RootAndOwner
            && req_uid != session.session_owner
            && !req_uid.is_root())
            || (session.allowed == SessionACL::Owner && req_uid != session.session_owner)
        {
            match self.operation {
                AsyncOperation::Init
                | AsyncOperation::Destroy
                | AsyncOperation::Read { .. }
                | AsyncOperation::ReadDir { .. } => {}
                _ => return Err(Errno::EACCES),
            }
        }

        let Some(filesystem) = &session.filesystem.fs else {
            error!("bug: filesystem must be initialized in dispatch_req");
            return Err(Errno::EIO);
        };

        match &self.operation {
            AsyncOperation::Init => {
                error!("Unexpected FUSE_INIT after handshake completed");
                Err(Errno::EIO)
            }
            AsyncOperation::Destroy => Err(Errno::EIO),
            AsyncOperation::Lookup { parent, name } => {
                let response = filesystem
                    .lookup(self.request_header(), *parent, name.as_os_str())
                    .await?;
                self.reply(&response).await
            }
            AsyncOperation::ReadDir {
                ino,
                file_handle,
                size,
                offset,
            } => {
                let response = filesystem
                    .readdir(self.request_header(), *ino, *file_handle, *size, *offset)
                    .await?;
                self.reply(&response).await
            }
            AsyncOperation::GetAttr { ino, file_handle } => {
                let response = filesystem
                    .getattr(self.request_header(), *ino, *file_handle)
                    .await?;
                self.reply(&response).await
            }
            AsyncOperation::Read {
                ino,
                file_handle,
                offset,
                size,
                flags,
                lock_owner,
            } => {
                let response = filesystem
                    .read(
                        self.request_header(),
                        *ino,
                        *file_handle,
                        *offset,
                        *size,
                        *flags,
                        *lock_owner,
                    )
                    .await?;
                self.reply(&response).await
            }
            AsyncOperation::Unsupported => {
                error!("Operation not implemented in the async dispatcher yet");
                Err(Errno::ENOSYS)
            }
        }
    }

    // Reply to the kernel with the given response payload, it should be called at most once per request.
    pub(crate) async fn reply<R: Response + Sync>(&self, response: &R) -> Result<(), Errno> {
        response
            .send_reply(&self.ch, self.request_header().unique())
            .await?;
        Ok(())
    }

    /// Returns a Request reference for this request
    #[inline]
    fn request_header(&self) -> &Request {
        Request::ref_cast(&self.header)
    }
}
