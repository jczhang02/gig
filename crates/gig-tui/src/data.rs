//! View models read from gig-core. Filled by a later ticket; for now the
//! snapshot is empty so the shell can refresh on its timer.

use gig_core::services::Ctx;
use gig_core::Result;

/// Everything the views draw, read in one pass on every refresh.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {}

impl Snapshot {
    pub fn load(_ctx: &Ctx) -> Result<Self> {
        Ok(Self::default())
    }
}
