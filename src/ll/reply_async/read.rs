//! Response data implementation of [`crate::AsyncFilesystem::read`] operation to
//! send to the kernel

use std::io::IoSlice;

use zerocopy::IntoBytes;

use crate::ll::{ioslice_concat::IosliceConcat, reply::Response};

/// Response data from [`crate::AsyncFilesystem::read`] operation
#[derive(Debug)]
pub struct ReadResponse {
    data: Vec<u8>,
}

impl ReadResponse {
    /// Creates a new [`ReadResponse`] with a specified buffer.
    pub fn new(data: Vec<u8>) -> ReadResponse {
        ReadResponse { data }
    }
}

impl Response for ReadResponse {
    fn payload(&self) -> impl IosliceConcat {
        [IoSlice::new(self.data.as_bytes())]
    }
}
