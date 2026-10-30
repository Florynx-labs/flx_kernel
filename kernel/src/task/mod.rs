//! Task management and scheduling policy.
//!
//! Architecture-specific context switching is intentionally kept outside this module.
pub mod scheduler;

pub use scheduler::{Scheduler, SchedulerError, Task, TaskId, TaskState};
