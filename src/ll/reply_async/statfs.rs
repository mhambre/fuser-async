//! Response data implementation of [`crate::AsyncFilesystem::statfs`] operation.

use crate::ll::ResponseStruct;
use crate::ll::ioslice_concat::IosliceConcat;
use crate::ll::reply::Response;

/// Response data from [`crate::AsyncFilesystem::statfs`] operation.
#[derive(Debug)]
pub struct StatfsResponse {
    blocks: u64,
    bfree: u64,
    bavail: u64,
    files: u64,
    ffree: u64,
    bsize: u32,
    namelen: u32,
    frsize: u32,
}

impl StatfsResponse {
    /// Creates a new [`StatfsResponse`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        blocks: u64,
        bfree: u64,
        bavail: u64,
        files: u64,
        ffree: u64,
        bsize: u32,
        namelen: u32,
        frsize: u32,
    ) -> Self {
        Self {
            blocks,
            bfree,
            bavail,
            files,
            ffree,
            bsize,
            namelen,
            frsize,
        }
    }
}

impl Response for StatfsResponse {
    fn payload(&self) -> impl IosliceConcat {
        ResponseStruct::new_statfs(
            self.blocks,
            self.bfree,
            self.bavail,
            self.files,
            self.ffree,
            self.bsize,
            self.namelen,
            self.frsize,
        )
    }
}
