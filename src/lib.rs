//! Switchboard: tracks coding-agent processes and tells the operator which
//! ones want them.
//!
//! Layering, from the outside in:
//!
//!   daemon/   tasks and sockets. The only place with tokio.
//!   ingest/   one pure adapter per source: raw bytes -> `Input`.
//!   reduce/   `apply_event`, `apply_snapshot`, `tick`. Pure, owns no I/O.
//!   model     the types. `Agent::phase` and `Shown::settle` live here.
//!
//! Leaves that every layer may use: `names` (identifier and vocabulary
//! conversions), `paths`, `error`, `time`, `tail`, `history`, `proc`, `view`.

// Scaffold: silences warnings from `todo!()` stubs. Delete in T15.
#![allow(unused)]

pub mod client;
pub mod daemon;
pub mod error;
pub mod history;
pub mod ingest;
pub mod model;
pub mod names;
pub mod paths;
pub mod proc;
pub mod reduce;
pub mod tail;
pub mod time;
pub mod usage;
pub mod view;
