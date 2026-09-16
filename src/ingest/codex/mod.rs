//! Codex: hooks only, for now.
//!
//! The app-server protocol (turn outcomes, approval ownership, the child
//! thread graph, account limits) is decision O1/O4 territory and is not
//! wired. Everything below is what `~/.codex/hooks.json` delivers on stdin.

pub mod hook;

pub use hook::{hook, join_input};
