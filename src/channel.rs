use std::io;
use std::os::fd::AsFd;
use std::os::fd::BorrowedFd;
use std::sync::Arc;

use nix::errno::Errno;

use crate::dev_fuse::DevFuse;
#[cfg(feature = "async-rust")]
use crate::lib_async::dev_fuse::AsyncDevFuse;
use crate::passthrough::BackingId;

/// A raw communication channel to the FUSE kernel driver
#[derive(Debug, Clone)]
pub(crate) struct Channel(Arc<DevFuse>);

impl AsFd for Channel {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl Channel {
    /// Create a new communication channel to the kernel driver by mounting the
    /// given path. The kernel driver will delegate filesystem operations of
    /// the given path to the channel.
    pub(crate) fn new(device: Arc<DevFuse>) -> Self {
        Self(device)
    }

    /// Receives data up to the capacity of the given buffer (can block).
    fn receive(&self, buffer: &mut [u8]) -> nix::Result<usize> {
        nix::unistd::read(&self.0, buffer)
    }

    /// Receives data up to the capacity of the given buffer (can block),
    /// retrying on errors that are safe to retry (ENOENT, EINTR, EAGAIN).
    ///
    /// - ENOENT: Operation interrupted. According to FUSE, this is safe to retry.
    /// - EINTR: Interrupted system call, retry.
    /// - EAGAIN: Explicitly instructed to try again.
    pub(crate) fn receive_retrying(&self, buffer: &mut [u8]) -> nix::Result<usize> {
        loop {
            match self.receive(buffer) {
                Ok(size) => return Ok(size),
                Err(Errno::ENOENT | Errno::EINTR | Errno::EAGAIN) => continue,
                Err(err) => return Err(err),
            }
        }
    }

    /// Returns a sender object for this channel. The sender object can be
    /// used to send to the channel. Multiple sender objects can be used
    /// and they can safely be sent to other threads.
    pub(crate) fn sender(&self) -> ChannelSender {
        // Since write/writev syscalls are threadsafe, we can simply create
        // a sender by using the same file and use it in other threads.
        ChannelSender(self.0.clone())
    }

    /// Clone the FUSE device fd using FUSE_DEV_IOC_CLONE ioctl.
    ///
    /// This creates a new fd that can read FUSE requests independently,
    /// enabling true parallel request processing. The kernel distributes
    /// requests across all cloned fds.
    ///
    /// Requires Linux 4.5+. Returns an error on older kernels or non-Linux.
    #[cfg(target_os = "linux")]
    pub(crate) fn clone_fd(&self) -> io::Result<Channel> {
        use std::os::fd::AsRawFd;

        let new_dev = DevFuse::open()?;

        let mut source_fd = self.0.as_raw_fd() as u32;
        // SAFETY: fuse_dev_ioc_clone is a valid ioctl for /dev/fuse
        unsafe {
            crate::ll::ioctl::fuse_dev_ioc_clone(new_dev.as_raw_fd(), &mut source_fd)?;
        }

        Ok(Channel::new(Arc::new(new_dev)))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ChannelSender(Arc<DevFuse>);

impl ChannelSender {
    pub(crate) fn send(&self, bufs: &[io::IoSlice<'_>]) -> io::Result<()> {
        let rc = nix::sys::uio::writev(&self.0, bufs)?;
        // writev is atomic, so do not need to check how many bytes are written.
        // libfuse does not do it either
        // https://github.com/libfuse/libfuse/blob/6278995cca991978abd25ebb2c20ebd3fc9e8a13/lib/fuse_lowlevel.c#L267
        debug_assert_eq!(bufs.iter().map(|b| b.len()).sum::<usize>(), rc);
        Ok(())
    }

    pub(crate) fn open_backing(&self, fd: BorrowedFd<'_>) -> std::io::Result<BackingId> {
        BackingId::create(&self.0, fd)
    }

    pub(crate) unsafe fn wrap_backing(&self, id: u32) -> BackingId {
        unsafe { BackingId::wrap_raw(&self.0, id) }
    }
}

/// A raw async communication channel to the FUSE kernel driver. This uses `AsyncDevFuse` under the hood and
/// provides async APIs for receiving data. This is used by the async Rust mount implementation.
#[cfg(feature = "async-rust")]
#[derive(Debug, Clone)]
pub(crate) struct AsyncChannel(Arc<AsyncDevFuse>);

impl AsFd for AsyncChannel {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl AsyncChannel {
    /// Create a new async communication channel to the kernel driver by mounting the
    /// given path. The kernel driver will delegate filesystem operations of
    /// the given path to the channel.
    pub(crate) fn new(device: Arc<AsyncDevFuse>) -> Self {
        Self(device)
    }

    /// Receives data up to the capacity of the given buffer.
    async fn receive(&self, buffer: &mut [u8]) -> tokio::io::Result<usize> {
        loop {
            let mut guard = self.0.0.readable().await?;
            match guard.try_io(|inner| {
                nix::unistd::read(inner.get_ref(), buffer)
                    .map_err(|e| io::Error::from_raw_os_error(e as i32))
            }) {
                Ok(result) => return result,
                Err(_would_block) => continue,
            }
        }
    }

    /// Receives data up to the capacity of the given buffer (can block),
    /// retrying on errors that are safe to retry (ENOENT, EINTR, EAGAIN).
    ///
    /// - ENOENT: Operation interrupted. According to FUSE, this is safe to retry.
    /// - EINTR: Interrupted system call, retry.
    /// - EAGAIN: Explicitly instructed to try again.
    pub(crate) async fn receive_retrying(&self, buffer: &mut [u8]) -> tokio::io::Result<usize> {
        loop {
            match self.receive(buffer).await {
                Ok(size) => return Ok(size),
                Err(e) => match nix::errno::Errno::from_raw(e.raw_os_error().unwrap_or(0)) {
                    Errno::ENOENT | Errno::EINTR | Errno::EAGAIN => continue,
                    _ => return Err(e),
                },
            }
        }
    }

    /// Returns a sender object for this channel. The sender object can be
    /// used to send to the channel. Multiple sender objects can be used
    /// and they can safely be sent to other threads.
    pub(crate) fn sender(&self) -> AsyncChannelSender {
        // Since write/writev syscalls are threadsafe, we can simply create
        // a sender by using the same file and use it in other threads.
        AsyncChannelSender(self.0.clone())
    }

    /// Clone the FUSE device fd using FUSE_DEV_IOC_CLONE ioctl.
    ///
    /// This creates a new fd that can read FUSE requests independently,
    /// enabling true parallel request processing. The kernel distributes
    /// requests across all cloned fds.
    ///
    /// Requires Linux 4.5+. Returns an error on older kernels or non-Linux.
    #[cfg(target_os = "linux")]
    pub(crate) async fn clone_fd(&self) -> tokio::io::Result<AsyncChannel> {
        // FUSE_DEV_IOC_CLONE requires a fresh /dev/fuse fd as the target.

        use std::os::fd::AsRawFd;
        let new_dev = AsyncDevFuse::open().await?;
        let mut source_fd = self.0.0.as_raw_fd() as u32;

        // SAFETY: fuse_dev_ioc_clone is a valid ioctl for /dev/fuse,
        // source_fd is valid for the lifetime of this call.
        unsafe {
            crate::ll::ioctl::fuse_dev_ioc_clone(new_dev.as_raw_fd(), &mut source_fd)
                .map_err(io::Error::from)?;
        }

        Ok(AsyncChannel::new(Arc::new(new_dev)))
    }
}

/// Object for sending data to an `AsyncChannel`. This can be safely cloned and sent to
/// other threads and is non-blocking.
#[cfg(feature = "async-rust")]
#[derive(Clone, Debug)]
pub(crate) struct AsyncChannelSender(Arc<AsyncDevFuse>);

impl AsyncChannelSender {
    pub(crate) async fn send(&self, bufs: &[io::IoSlice<'_>]) -> tokio::io::Result<()> {
        loop {
            let mut guard = self.0.0.writable().await?;
            match guard.try_io(|inner| {
                nix::sys::uio::writev(inner.get_ref(), bufs).map_err(io::Error::from)
            }) {
                Ok(rc) => {
                    debug_assert_eq!(bufs.iter().map(|b| b.len()).sum::<usize>(), rc?);
                    return Ok(());
                }
                Err(_would_block) => continue,
            }
        }
    }
}
