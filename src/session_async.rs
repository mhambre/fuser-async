//! Filesystem session
//!
//! A session runs a filesystem implementation while it is being mounted to a specific mount
//! point. A session begins by mounting the filesystem and ends by unmounting it. While the
//! filesystem is mounted, the session loop receives, dispatches and replies to kernel requests
//! for filesystem operations under its mount point.

use std::io;
use std::os::fd::AsFd;
use std::os::fd::BorrowedFd;
use std::path::Path;
use std::sync::Arc;

use log::warn;
use nix::unistd::Uid;
use nix::unistd::geteuid;

use crate::Config;
use crate::KernelConfig;
use crate::MountOption;
use crate::Request;
use crate::SessionACL;
use crate::channel_async::AsyncChannel;
use crate::lib_async::AsyncFilesystem;
use crate::ll;
use crate::ll::AnyRequest;
use crate::ll::Version;
use crate::ll::fuse_abi as abi;
use crate::ll::reply::Response;
use crate::ll::request_async::AsyncRequestWithSender;
use crate::mnt::AsyncMount;
use crate::mnt::mount_options::check_option_conflicts;
use crate::read_buf::FuseReadBuf;
use crate::session::MAX_WRITE_SIZE;
use parking_lot::Mutex;

type DropTx<T> = Arc<Mutex<Option<tokio::sync::mpsc::Sender<T>>>>;

/// Calls `destroy` on drop.
#[derive(Debug)]
pub(crate) struct AsyncSessionGuard<FS: AsyncFilesystem> {
    pub(crate) fs: Option<FS>,
    pub(crate) unmount_tx: DropTx<()>,
}

/// Calls `destroy` on drop to do any user-specific cleanup.
impl<FS: AsyncFilesystem> AsyncSessionGuard<FS> {
    fn destroy(&mut self) {
        if let Some(tx) = self.unmount_tx.lock().take() {
            tx.try_send(()).ok();
        }
        if let Some(mut fs) = self.fs.take() {
            fs.destroy();
        }
    }
}

/// Calls `destroy` on drop to do any user-specific cleanup.
impl<FS: AsyncFilesystem> Drop for AsyncSessionGuard<FS> {
    fn drop(&mut self) {
        self.destroy();
    }
}

/// Builder for [`AsyncSession`]. This is used to construct an instance of [`AsyncSession`]
/// within an asynchronous context.
#[derive(Default, Debug)]
pub struct AsyncSessionBuilder<FS: AsyncFilesystem> {
    filesystem: Option<FS>,
    mountpoint: Option<String>,
    options: Option<Config>,
}

impl<FS: AsyncFilesystem> AsyncSessionBuilder<FS> {
    /// Create a new builder for [`AsyncSession`].
    pub fn new() -> Self {
        Self {
            filesystem: None,
            mountpoint: None,
            options: None,
        }
    }

    /// Set the filesystem implementation for this session. This is required.
    pub fn filesystem(mut self, fs: FS) -> Self {
        self.filesystem = Some(fs);
        self
    }

    /// Set the mountpoint for this session. This is required.
    pub fn mountpoint(mut self, mountpoint: impl AsRef<Path>) -> Self {
        self.mountpoint = Some(mountpoint.as_ref().to_string_lossy().to_string());
        self
    }

    /// Set the options for this session. This is required.
    pub fn options(mut self, options: Config) -> io::Result<Self> {
        check_option_conflicts(&options)?;

        // validate permissions options
        if options.mount_options.contains(&MountOption::AutoUnmount)
            && options.acl == SessionACL::Owner
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "auto_unmount requires acl != Owner".to_string(),
            ));
        }

        self.options = Some(options);
        Ok(self)
    }

    /// Build the session. This will mount the filesystem and return an `AsyncSession` if successful.
    pub async fn build(self) -> io::Result<AsyncSession<FS>> {
        let filesystem = self.filesystem.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "`filesystem` is required")
        })?;
        let mountpoint = self.mountpoint.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "`mountpoint` is required")
        })?;
        let options = self
            .options
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "`options` are required"))?;

        AsyncSession::init(filesystem, mountpoint, &options).await
    }
}

/// The async session data structure
#[derive(Debug)]
pub struct AsyncSession<FS: AsyncFilesystem> {
    /// Filesystem operation access and drop guard.
    pub(crate) guard: AsyncSessionGuard<FS>,
    /// Communication channel to the kernel driver
    pub(crate) ch: AsyncChannel,
    /// Whether to restrict access to owner, root + owner, or unrestricted
    /// Used to implement `allow_root` and `auto_unmount`
    pub(crate) allowed: SessionACL,
    /// User that launched the fuser process
    pub(crate) session_owner: Uid,
    /// FUSE protocol version, as reported by the kernel.
    /// The field is set to `Some` when the init message is received.
    pub(crate) proto_version: Option<Version>,
    /// Config options for this session, used for debugging and for
    /// feature gating in the future.
    pub(crate) config: Config,
}

/// A session that is running in the background. The filesystem is unmounted when
/// the session ends.
impl<FS: AsyncFilesystem> AsFd for AsyncSession<FS> {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.ch.as_fd()
    }
}

impl<FS: AsyncFilesystem> AsyncSession<FS> {
    /// Create a new session and mount the given async filesystem to the given mountpoint.
    ///
    /// # Errors
    /// Returns an error if the options are incorrect, or if the fuse device can't be mounted.
    async fn init<P: AsRef<Path>>(
        filesystem: FS,
        mountpoint: P,
        options: &Config,
    ) -> io::Result<Self> {
        let mountpoint = mountpoint.as_ref();

        // mount (async)
        let mut mount = AsyncMount::new();
        mount = mount
            .mount(mountpoint, &options.mount_options, options.acl)
            .await?;
        let file = mount.dev_fuse().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Other,
                "Failed to get /dev/fuse file descriptor from mount",
            )
        })?;
        let ch = AsyncChannel::new(file.clone());

        // mount drop guard
        let (unmount_tx, mut unmount_rx) = tokio::sync::mpsc::channel::<()>(1);
        tokio::spawn({
            let mount = Arc::new(Mutex::new(Some(mount)));
            async move {
                // Wait for the signal to unmount
                let _ = unmount_rx.recv().await;
                if let Some(mount) = mount.lock().take() {
                    drop(mount);
                }
            }
        });

        let mut session = AsyncSession {
            guard: AsyncSessionGuard {
                fs: Some(filesystem),
                unmount_tx: Arc::new(Mutex::new(Some(unmount_tx))),
            },
            ch,
            allowed: options.acl,
            session_owner: geteuid(),
            proto_version: None,
            config: options.clone(),
        };

        session.handshake().await?;

        Ok(session)
    }

    /// Run the session async loop that receives kernel requests and dispatches them to method
    /// calls into the filesystem.
    ///
    /// # Errors
    /// Returns any final error when the session comes to an end.
    pub async fn run(self) -> io::Result<()> {
        let AsyncSession {
            guard,
            ch,
            allowed,
            session_owner,
            proto_version: _,
            config,
        } = self;

        let n_threads = config.n_threads.unwrap_or(1);
        if n_threads == 0 {
            return Err(io::Error::other("n_threads"));
        }
        let Some(n_threads_minus_one) = n_threads.checked_sub(1) else {
            return Err(io::Error::other("n_threads"));
        };

        let filesystem = Arc::new(guard);

        // Give each individual thread its own channel by cloning or using `FUSE_DEV_IOC_CLONE` if requested,
        // which allows for more efficient request processing when multiple threads are used.
        let mut channels = Vec::with_capacity(n_threads);
        for _ in 0..n_threads_minus_one {
            if config.clone_fd {
                #[cfg(target_os = "linux")]
                {
                    channels.push(ch._clone_fd().await?);
                    continue;
                }
                #[cfg(not(target_os = "linux"))]
                {
                    return Err(io::Error::other("clone_fd is only supported on Linux"));
                }
            } else {
                channels.push(ch.clone());
            }
        }
        channels.push(ch);

        // Construct the event loop for each thread.
        let mut tasks = Vec::with_capacity(n_threads);
        for (i, ch) in channels.into_iter().enumerate() {
            let thread_name = format!("fuser-async-{i}");
            let event_loop = AsyncSessionEventLoop {
                thread_name: thread_name.clone(),
                filesystem: filesystem.clone(),
                ch,
                allowed,
                session_owner,
            };
            tasks.push(tokio::spawn(async move { event_loop.event_loop().await }));
        }

        // Wait for all event loop tasks to finish (shouldn't happen), and return the first error
        // if any of them fail.
        let mut reply: io::Result<()> = Ok(());
        for task in tasks {
            let res = match task.await {
                Ok(res) => res,
                Err(_) => {
                    return Err(io::Error::other("event loop task panicked"));
                }
            };
            if let Err(e) = res {
                if reply.is_ok() {
                    reply = Err(e);
                }
            }
        }

        reply
    }

    /// Perform the initial handshake with the kernel, which involves receiving the init message,
    async fn handshake(&mut self) -> io::Result<()> {
        let mut buf = vec![0u8; MAX_WRITE_SIZE];
        let sender = self.ch.sender();
        let Some(fs) = &mut self.guard.fs else {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Filesystem was not available during handshake",
            ));
        };

        // Keep checking for an init message from the kernel until we get one with a supported version,
        // at which point we reply to finish the handshake and return
        loop {
            let size = self
                .ch
                .receive_retrying(&mut buf)
                .await
                .map_err(|e| io::Error::new(e.kind(), format!("receive_retrying: {e}")))?;
            let request = AnyRequest::try_from(&buf[..size])
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

            // Convert the handshake request from the kernel to a usable operation
            let Ok(ll::Operation::Init(init)) = request.operation() else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "expected FUSE_INIT",
                ));
            };
            let v = init.version();

            // Validate version support
            if v.0 > abi::FUSE_KERNEL_VERSION {
                init.reply_version_only()
                    .send_reply(&sender, request.unique())
                    .await?;
                continue;
            }
            if v < Version(7, 6) {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "FUSE ABI too old",
                ));
            }

            // Construct kernel config from the init message user init() implementation and reply with it to finish
            // the handshake
            let mut config = KernelConfig::new(init.capabilities(), init.max_readahead(), v);
            fs.init(Request::ref_cast(request.header()), &mut config)
                .await
                .map_err(|e| io::Error::new(e.kind(), format!("fs.init: {e}")))?;
            self.proto_version = Some(v);
            let response = init.reply(&config);
            response
                .send_reply(&sender, request.unique())
                .await
                .map_err(|e| io::Error::new(e.kind(), format!("send init reply: {e}")))?;
            return Ok(());
        }
    }
}

pub(crate) struct AsyncSessionEventLoop<FS: AsyncFilesystem> {
    /// Cache thread name for faster `debug!`.
    pub(crate) thread_name: String,
    pub(crate) ch: AsyncChannel,
    pub(crate) filesystem: Arc<AsyncSessionGuard<FS>>,
    pub(crate) allowed: SessionACL,
    pub(crate) session_owner: Uid,
}

impl<FS: AsyncFilesystem> Clone for AsyncSessionEventLoop<FS> {
    fn clone(&self) -> Self {
        Self {
            thread_name: self.thread_name.clone(),
            ch: self.ch.clone(),
            filesystem: self.filesystem.clone(),
            allowed: self.allowed,
            session_owner: self.session_owner,
        }
    }
}

impl<FS: AsyncFilesystem> AsyncSessionEventLoop<FS> {
    async fn event_loop(&self) -> io::Result<()> {
        let mut buf = FuseReadBuf::new();
        let buf = buf.as_mut();

        loop {
            let resp_size = self.ch.receive_retrying(buf).await?;
            let sender = self.ch.sender();
            let session = self.clone();
            if let Ok(request) = AsyncRequestWithSender::new(sender, &buf[..resp_size]) {
                tokio::spawn(async move {
                    request.dispatch(&session).await;
                });
            } else {
                warn!("Received invalid request, skipping...");
            }
        }
    }
}
