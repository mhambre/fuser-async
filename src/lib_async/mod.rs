//! Experimental Asynchronous API for fuser. This is gated behind the "async" feature,
//! and is not yet considered stable. The API may change without a major version bump.

pub(crate) mod dev_fuse;
pub(crate) mod session;

use std::path::Path;

use tokio::io;

use crate::Config;

#[async_trait::async_trait]
pub trait AsyncFilesystem: Send + Sync + 'static {
    /// Clean up filesystem.
    /// Called on filesystem exit.
    fn destroy(&mut self) {}
}

/// Mount the given async filesystem to the given mountpoint. This function will
/// not return until the filesystem is unmounted.
///
/// # Errors
/// Returns an error if the options are incorrect, or if the fuse device can't be mounted,
/// and any final error when the session comes to an end.
async fn mount_async<FS: AsyncFilesystem, P: AsRef<Path>>(
    filesystem: FS,
    mountpoint: P,
    options: &Config,
) -> io::Result<()> {
    // Session::new(filesystem, mountpoint.as_ref(), options).and_then(session::Session::run)
    unimplemented!("")
}
