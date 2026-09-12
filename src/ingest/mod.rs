//! Adapters: raw source data in, `Input` out.
//!
//! Every adapter is a pure function. No state, no I/O, no ordering
//! assumptions, and no phases: an adapter says what happened and who caused
//! it, and the reducer decides what that means.
//!
//! Tolerance is the other rule. Harness releases ship daily, so an adapter
//! that meets a field it does not understand skips that fact rather than the
//! whole payload, and an event it does not model produces nothing.

use std::time::Instant;

use crate::model::{Event, ProcKey, Snapshot};

pub mod claude;
pub mod forward;
pub mod frame;
pub mod hash;
pub mod json;

/// What the reducer consumes.
#[derive(Debug, Clone)]
pub enum Input {
    Event(Event),
    Snapshot(Snapshot),
}

/// What an adapter is told about the world besides the payload: the local
/// clock reading at receipt, and which agent process sent it, when known.
#[derive(Debug, Clone, Copy)]
pub struct Ctx {
    pub now: Instant,
    pub process: Option<ProcKey>,
}
