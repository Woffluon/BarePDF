#![forbid(unsafe_code)]

pub mod cache;
mod error;
pub mod memory_budget;
mod observability;
pub mod protocol;
mod queue;
pub mod scheduler;
mod worker;

pub use cache::{BitmapCache, CacheKey};
pub use error::RenderError;
pub use memory_budget::SystemHardwareProfile;
pub use protocol::{
    PageRange, Priority, RenderCommand, RenderEvent, RenderJob, RenderKind, SearchMatchRecord,
};
pub use scheduler::RenderScheduler;
