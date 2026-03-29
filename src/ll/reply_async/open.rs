//! Response data implementation of async open-style operations.

use crate::FopenFlags;
use crate::ll::FileHandle;
use crate::ll::ResponseStruct;
use crate::ll::ioslice_concat::IosliceConcat;
use crate::ll::reply::Response;

/// Response data from async open-style operations.
#[derive(Debug)]
pub struct OpenResponse {
    fh: FileHandle,
    flags: FopenFlags,
}

impl OpenResponse {
    /// Creates a new [`OpenResponse`].
    pub fn new(fh: FileHandle, flags: FopenFlags) -> Self {
        Self { fh, flags }
    }
}

impl Response for OpenResponse {
    fn payload(&self) -> impl IosliceConcat {
        ResponseStruct::new_open(self.fh, self.flags, 0)
    }
}
