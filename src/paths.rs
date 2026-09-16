//! Where things live on disk, as pure functions of their inputs.
//!
//! Nothing here touches the filesystem. Callers pass the roots in, which is
//! what makes the daemon testable against a temporary directory.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::model::{Agent, SessionId};

/// Claude's project directory name for a working directory: every byte that
/// is not an ASCII letter or digit becomes `-`.
///
/// `/home/u/Projects/switchboard-rust` -> `-home-u-Projects-switchboard-rust`
/// `/home/u/.claude`                   -> `-home-u--claude`
///
/// A non-UTF-8 path is slugged from its lossy form, which only ever replaces
/// characters that would have become `-` anyway.
pub fn cwd_slug(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// `<projects>/<cwd-slug>/<session>.jsonl`, where `projects` is normally
/// `~/.claude/projects`.
pub fn transcript_path(projects: &Path, cwd: &Path, session: &SessionId) -> PathBuf {
    projects
        .join(cwd_slug(cwd))
        .join(format!("{session}.jsonl"))
}

/// The last component of a working directory, when it is valid UTF-8.
///
/// Borrows from `cwd`: the returned `&str` lives exactly as long as the path
/// it was read from.
pub fn workspace_name(cwd: &Path) -> Option<&str> {
    cwd.file_name()?.to_str()
}

/// The name a chip shows: the session title, else the workspace, else the
/// last component of the cwd, else `"?"`.
pub fn display_name(agent: &Agent) -> &str {
    agent
        .title
        .as_deref()
        .or(agent.workspace.as_deref())
        .or_else(|| workspace_name(&agent.cwd))
        .unwrap_or("?")
}

/// `$HOME`, or `None` when it is unset or empty.
pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// The daemon socket: `$SWITCHBOARD_RUST_SOCKET`, else
/// `$XDG_RUNTIME_DIR/switchboard-rust.sock`, else the same name in the
/// system temporary directory. Shared by the daemon and the forwarder, which
/// must agree without any other configuration.
pub fn socket_path() -> PathBuf {
    let nonempty = |var: &str| std::env::var_os(var).filter(|v| !v.is_empty());
    nonempty("SWITCHBOARD_RUST_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            nonempty("XDG_RUNTIME_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join("switchboard-rust.sock")
        })
}

/// `state.json` -> `state.json.tmp`, in the same directory so the final
/// `rename` stays on one filesystem. `None` for a path with no file name.
pub fn temp_sibling(path: &Path) -> Option<PathBuf> {
    let mut name = path.file_name()?.to_os_string();
    name.push(".tmp");
    Some(path.with_file_name(name))
}

/// Every well-known location, resolved once at startup from the environment.
#[derive(Debug, Clone)]
pub struct Layout {
    pub proc_root: PathBuf,
    pub claude_projects: PathBuf,
    pub claude_sessions: PathBuf,
    pub socket: PathBuf,
    pub state_file: PathBuf,
    pub history_dir: PathBuf,
}

impl Layout {
    /// `None` when `$HOME` is unset.
    pub fn from_env() -> Option<Layout> {
        let home = home()?;
        let xdg = |var: &str, fallback: &str| {
            std::env::var_os(var)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(fallback))
        };
        Some(Layout {
            proc_root: PathBuf::from("/proc"),
            claude_projects: home.join(".claude/projects"),
            claude_sessions: home.join(".claude/sessions"),
            socket: socket_path(),
            state_file: xdg("XDG_CACHE_HOME", ".cache").join("switchboard-rust/state.json"),
            history_dir: xdg("XDG_STATE_HOME", ".local/state").join("switchboard-rust/history"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_replace_every_non_alphanumeric_byte_when_slugging_a_cwd() {
        assert_eq!(
            cwd_slug(Path::new(
                "/home/u/Projects/Arachne/.worktrees/feat/f90-drain"
            )),
            "-home-u-Projects-Arachne--worktrees-feat-f90-drain"
        );
    }

    #[test]
    fn should_return_none_when_the_cwd_has_no_final_component() {
        assert_eq!(workspace_name(Path::new("/")), None);
        assert_eq!(workspace_name(Path::new("/a/b")), Some("b"));
    }
}
