//! Asynchronous reply types for FUSE operations. These are used to send responses
//! back to the kernel for requests through method returns in [`crate::AsyncFilesystem`].

mod getattr;
mod lookup;
mod read;
mod readdir;

pub use getattr::GetAttrResponse;
pub use lookup::LookupResponse;
pub use read::ReadResponse;
pub use readdir::DirectoryResponse;

use std::future::Future;
use std::pin::Pin;

use crate::channel_async::AsyncChannelSender;
use crate::ll::RequestId;
use crate::ll::reply::Response;

/// Trait for asynchronous responses that can be sent back to the kernel. This is implemented
/// for all types that implement [`crate::ll::reply::Response`] so that they can be sent asynchronously
/// through an [`AsyncChannelSender`].
pub(crate) trait AsyncResponse: Send + Sync {
    fn send_reply_async<'a>(
        &'a self,
        sender: &'a AsyncChannelSender,
        unique: RequestId,
    ) -> Pin<Box<dyn Future<Output = Result<(), tokio::io::Error>> + 'a>>;
}

/// Blanket implementation of [`AsyncResponse`] for all types that implement [`Response`], allowing them
/// to be sent asynchronously.
impl<T> AsyncResponse for T
where
    T: Response + Send + Sync,
{
    fn send_reply_async<'a>(
        &'a self,
        sender: &'a AsyncChannelSender,
        unique: RequestId,
    ) -> Pin<Box<dyn Future<Output = Result<(), tokio::io::Error>> + 'a>> {
        Box::pin(async move { self.send_reply(sender, unique).await })
    }
}
