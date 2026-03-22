//! Filesystem operation request
//!
//! A request represents information about a filesystem operation the kernel driver wants us to
//! perform.

use std::convert::TryFrom;

use log::debug;
use log::error;

use crate::AsyncFilesystem;
use crate::Request;
use crate::channel_async::AsyncChannelSender;
use crate::ll;
use crate::ll::Errno;
use crate::ll::ResponseErrno;
use crate::ll::reply_async::AsyncResponse;
use crate::session::SessionACL;
use crate::session_async::AsyncSessionEventLoop;

/// Helper to convert response returns from T: AsyncFilesystem to Some(Box<dyn AsyncResponse>)
/// for dispatching
fn opt_boxed<T: AsyncResponse + 'static>(r: T) -> Option<Box<dyn AsyncResponse>> {
    Some(Box::new(r))
}

/// Asynchronous request data structure (request from the kernel along with a
/// channel sender clone for sending the reply).
#[derive(Debug)]
pub(crate) struct AsyncRequestWithSender<'a> {
    /// Async sender for sending the reply
    ch: AsyncChannelSender,
    /// Parsed request
    pub(crate) request: ll::AnyRequest<'a>,
}

impl<'a> AsyncRequestWithSender<'a> {
    /// Create a new request from the given data
    pub(crate) fn new(
        ch: AsyncChannelSender,
        data: &'a [u8],
    ) -> Result<AsyncRequestWithSender<'a>, tokio::io::Error> {
        // Validate request is formed properly
        let request = ll::AnyRequest::try_from(data).map_err(|err| {
            error!("Failed to parse request from kernel: {}", err);
            tokio::io::Error::new(tokio::io::ErrorKind::InvalidData, "Failed to parse request")
        })?;

        Ok(Self { ch, request })
    }

    /// Async dispatch request to the given filesystem, takes the return from the filesystem and
    /// sends the reply back to the kernel. This follows a more Rust-idiomatic async API design rather than the C-like,
    /// callback-based interface used in [`crate::Filesystem`].
    pub(crate) async fn dispatch<FS: AsyncFilesystem>(&self, session: &AsyncSessionEventLoop<FS>) {
        debug!("{} thread={}", self.request, session.thread_name);
        match self.dispatch_req(session).await {
            Ok(Some(response)) => self.reply(response.as_ref()).await.unwrap_or_else(|e| {
                error!("Failed to send reply for request {}: {e:?}", self.request);
            }),
            Ok(None) => {}
            Err(errno) => {
                let response = ResponseErrno(errno);
                self.reply(&response).await.unwrap_or_else(|e| {
                    error!(
                        "Failed to send error reply for request {}: {e:?}",
                        self.request
                    );
                })
            }
        }
    }

    /// Dispatch the request to the given filesystem method and return the response in a shared format to be sent
    /// back to the kernel.
    async fn dispatch_req<FS: AsyncFilesystem>(
        &self,
        session: &AsyncSessionEventLoop<FS>,
    ) -> Result<Option<Box<dyn crate::ll::reply_async::AsyncResponse>>, Errno> {
        let op = self.request.operation().map_err(|_| Errno::ENOSYS)?;

        // Implement allow_root & access check for auto_unmount
        if (session.allowed == SessionACL::RootAndOwner
            && self.request.uid() != session.session_owner
            && !self.request.uid().is_root())
            || (session.allowed == SessionACL::Owner && self.request.uid() != session.session_owner)
        {
            {
                match op {
                    // Only allow operations that the kernel may issue without a uid set
                    ll::Operation::Init(_)
                    | ll::Operation::Destroy(_)
                    | ll::Operation::Read(_)
                    | ll::Operation::ReadDir(_) => {}
                    _ => {
                        return Err(Errno::EACCES);
                    }
                }
            }
        }

        let Some(filesystem) = &session.filesystem.fs else {
            // This is handled before dispatch call.
            error!("bug: filesystem must be initialized in dispatch_req");
            return Err(Errno::EIO);
        };

        Ok(match op {
            ll::Operation::Init(_) => {
                // Cannot allow reinit after handshake is completed
                error!("Unexpected FUSE_INIT after handshake completed");
                return Err(Errno::EIO);
            }
            ll::Operation::Destroy(_) => {
                // This is handled before dispatch call and
                // should not happen here.
                return Err(Errno::EIO);
            }
            ll::Operation::Lookup(x) => opt_boxed(
                filesystem
                    .lookup(
                        self.request_header(),
                        self.request.nodeid(),
                        x.name().as_os_str(),
                    )
                    .await?,
            ),
            ll::Operation::ReadDir(x) => opt_boxed(
                filesystem
                    .readdir(
                        self.request_header(),
                        self.request.nodeid(),
                        x.file_handle(),
                        x.offset(),
                    )
                    .await?,
            ),
            ll::Operation::GetAttr(x) => opt_boxed(
                filesystem
                    .getattr(
                        self.request_header(),
                        self.request.nodeid(),
                        x.file_handle(),
                    )
                    .await?,
            ),
            ll::Operation::Read(x) => opt_boxed(
                filesystem
                    .read(
                        self.request_header(),
                        self.request.nodeid(),
                        x.file_handle(),
                        x.offset()?,
                        x.size(),
                        x.flags(),
                        x.lock_owner(),
                    )
                    .await?,
            ),
            _ => {
                error!(
                    "Operation {} not implemented in the async dispatcher yet",
                    op
                );
                return Err(Errno::ENOSYS);
            }
        })
    }

    // Reply to the kernel with the given response payload, it should be called at most once per request.
    pub(crate) async fn reply(&self, response: &dyn AsyncResponse) -> Result<(), Errno> {
        let ch = self.ch.clone();
        response
            .send_reply_async(&ch, self.request.unique())
            .await?;
        Ok(())
    }

    /// Returns a Request reference for this request
    #[inline]
    fn request_header(&self) -> &Request {
        Request::ref_cast(self.request.header())
    }
}
