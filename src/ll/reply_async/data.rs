//! Response data implementation for operations that return an arbitrary byte
//! buffer.

use std::io::IoSlice;

use crate::ll::{ioslice_concat::IosliceConcat, reply::Response};

/// Response data from async operations that return a byte buffer.
#[derive(Debug)]
pub struct DataResponse {
    data: Vec<u8>,
}

impl DataResponse {
    /// Creates a new [`DataResponse`] with the provided bytes.
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    fn bytes(&self) -> &[u8] {
        self.data.as_slice()
    }
}

impl Response for DataResponse {
    fn payload(&self) -> impl IosliceConcat {
        [IoSlice::new(self.bytes())]
    }
}
