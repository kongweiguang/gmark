// @author kongweiguang

//! Per-document recovery coordination.
//!
//! The state and execution pieces are split into focused submodules so the
//! Controller-facing recovery surface stays reviewable and no journal I/O can
//! re-enter the Controller mutex.

#[path = "worker_host.rs"]
mod worker_host;
#[path = "worker_parts.rs"]
mod worker_parts;

pub(super) use worker_parts::{
    RecoveryFlushStatus, RecoveryJob, RecoveryQueueError, RecoveryWorker, SharedRecoveryState,
};
