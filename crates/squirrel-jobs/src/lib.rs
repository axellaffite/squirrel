//! A Postgres job queue (`jobs` table, see migration 20261114000003) and the
//! worker runtime that executes it.

pub mod admin;
mod backoff;
mod job;
pub mod queue;
mod worker;

pub use backoff::{jitter, retry_delay};
pub use job::{JobContext, JobError, NewJob, priority};
pub use queue::enqueue;
pub use worker::{Registry, Worker, WorkerConfig};
