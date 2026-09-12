//! Claude Code: hooks, transcript lines, and the status line.

pub mod hook;
pub mod statusline;
pub mod transcript;

pub use hook::hook;
pub use statusline::statusline;
pub use transcript::{LineCtx, line};
