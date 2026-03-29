//! Response data implementation of [`crate::AsyncFilesystem::create`] operation.

use std::time::Duration;

use crate::FileAttr;
use crate::FopenFlags;
use crate::Generation;
use crate::ll::FileHandle;
use crate::ll::ResponseStruct;
use crate::ll::ioslice_concat::IosliceConcat;
use crate::ll::reply::Attr;
use crate::ll::reply::Response;

/// Response data from [`crate::AsyncFilesystem::create`] operation.
#[derive(Debug)]
pub struct CreateResponse {
    ttl: Duration,
    attr: FileAttr,
    generation: Generation,
    fh: FileHandle,
    flags: FopenFlags,
}

impl CreateResponse {
    /// Creates a new [`CreateResponse`].
    pub fn new(
        ttl: Duration,
        attr: FileAttr,
        generation: Generation,
        fh: FileHandle,
        flags: FopenFlags,
    ) -> Self {
        Self {
            ttl,
            attr,
            generation,
            fh,
            flags,
        }
    }
}

impl Response for CreateResponse {
    fn payload(&self) -> impl IosliceConcat {
        ResponseStruct::new_create(
            &self.ttl,
            &Attr::from(self.attr),
            self.generation,
            self.fh,
            self.flags,
            0,
        )
    }
}
