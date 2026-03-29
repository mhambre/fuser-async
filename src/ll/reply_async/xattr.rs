//! Response data implementation of async xattr operations.

use std::io::IoSlice;

use smallvec::SmallVec;

use crate::ll::ResponseStruct;
use crate::ll::fuse_abi;
use crate::ll::ioslice_concat::IosliceConcat;
use crate::ll::reply::Response;

enum XattrPayload<'a> {
    Size(ResponseStruct<fuse_abi::fuse_getxattr_out>),
    Data([IoSlice<'a>; 1]),
}

impl IosliceConcat for XattrPayload<'_> {
    fn iter_slices(&self) -> impl Iterator<Item = IoSlice<'_>> + '_ {
        let slices = match self {
            Self::Size(size) => SmallVec::<[IoSlice<'_>; 1]>::from_iter(size.iter_slices()),
            Self::Data(data) => SmallVec::<[IoSlice<'_>; 1]>::from_iter(data.iter_slices()),
        };
        slices.into_iter()
    }
}

/// Response data from async xattr operations.
#[derive(Debug)]
pub enum XattrResponse {
    /// Report the size of the xattr payload.
    Size(u32),
    /// Return the xattr payload bytes.
    Data(Vec<u8>),
}

impl XattrResponse {
    /// Creates a size-reporting xattr response.
    pub fn size(size: u32) -> Self {
        Self::Size(size)
    }

    /// Creates a data xattr response.
    pub fn data(data: Vec<u8>) -> Self {
        Self::Data(data)
    }
}

impl Response for XattrResponse {
    fn payload(&self) -> impl IosliceConcat {
        match self {
            Self::Size(size) => XattrPayload::Size(ResponseStruct::new_xattr_size(*size)),
            Self::Data(data) => XattrPayload::Data([IoSlice::new(data.as_slice())]),
        }
    }
}
