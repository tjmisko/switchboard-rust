//! T01 — identifier newtypes and borrowed string/path types.

mod common;

use std::borrow::Borrow;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use switchboard::model::*;
use switchboard::paths::{cwd_slug, display_name, temp_sibling, transcript_path, workspace_name};

fn takes_str(s: &str) -> &str {
    s
}

#[test]
fn should_pass_an_id_reference_where_a_str_is_expected_when_deref_is_implemented() {
    let id = ToolUseId::from("toolu_01");
    // `&ToolUseId` -> `&str` by deref coercion.
    assert_eq!(takes_str(&id), "toolu_01");
    // `str` methods are reachable through auto-deref.
    assert!(id.starts_with("toolu_"));
    assert_eq!(id.len(), 8);
}

#[test]
fn should_expose_the_same_str_through_as_str_as_ref_and_borrow() {
    let id = SessionId::from("s-1");
    let via_as_ref: &str = id.as_ref();
    let via_borrow: &str = id.borrow();
    assert_eq!(id.as_str(), "s-1");
    assert_eq!(via_as_ref, "s-1");
    assert_eq!(via_borrow, "s-1");
}

#[test]
fn should_look_up_a_btreemap_and_a_hashmap_by_str_when_borrow_is_implemented() {
    let mut ordered = BTreeMap::new();
    ordered.insert(SubagentId::from("a1"), 1);
    let mut hashed = HashMap::new();
    hashed.insert(ToolName::from("Bash"), 2);

    assert_eq!(ordered.get("a1"), Some(&1));
    assert_eq!(hashed.get("Bash"), Some(&2));
    assert_eq!(hashed.get("bash"), None);
}

#[test]
fn should_turn_an_optional_id_into_an_optional_str_with_as_deref() {
    // `Option<SessionId>` -> `Option<&str>`: `as_deref` goes through `Deref`.
    let some: Option<SessionId> = Some("s".into());
    let none: Option<SessionId> = None;
    assert_eq!(some.as_deref(), Some("s"));
    assert_eq!(none.as_deref(), None);
}

#[test]
fn should_display_as_the_bare_identifier() {
    let id = PromptId::from("9b1c");
    assert_eq!(format!("{id}"), "9b1c");
    assert_eq!(id.to_string(), "9b1c");
}

#[test]
fn should_strip_agent_exactly_once_when_reading_a_subagent_file_stem() {
    let cases = [
        ("agent-a1a4f5cc80cc6d9c2", Some("a1a4f5cc80cc6d9c2")),
        ("agent-agent-x", Some("agent-x")),
        ("agent-", None),
        ("a1a4f5cc80cc6d9c2", None),
        ("", None),
    ];
    for (stem, expected) in cases {
        assert_eq!(
            SubagentId::from_file_stem(stem).as_deref(),
            expected,
            "stem {stem:?}"
        );
    }
}

#[test]
fn should_take_a_hook_agent_id_as_is_but_reject_blank_ones() {
    assert_eq!(
        SubagentId::from_hook("agent-child").as_deref(),
        Some("agent-child")
    );
    assert_eq!(SubagentId::from_hook("  a1  ").as_deref(), Some("a1"));
    assert_eq!(SubagentId::from_hook("   "), None);
    assert_eq!(SubagentId::from_hook(""), None);
}

#[test]
fn should_slug_every_non_alphanumeric_character_when_naming_a_project_dir() {
    let cases = [
        (
            "/home/u/Projects/switchboard-rust",
            "-home-u-Projects-switchboard-rust",
        ),
        ("/home/u/.claude", "-home-u--claude"),
        (
            "/home/u/Projects/Arachne/.worktrees/feat/f90-drain",
            "-home-u-Projects-Arachne--worktrees-feat-f90-drain",
        ),
        ("/tmp/snake_case dir", "-tmp-snake-case-dir"),
    ];
    for (cwd, slug) in cases {
        assert_eq!(cwd_slug(Path::new(cwd)), slug);
    }
}

/// `é` is one `char` and two bytes. The slug replaces characters, so it
/// becomes one `-`, not two.
#[test]
fn should_replace_a_multibyte_character_with_one_dash_when_slugging_a_cwd() {
    assert_eq!(cwd_slug(Path::new("/home/u/café")), "-home-u-caf-");
    assert_eq!(cwd_slug(Path::new("/home/u/日本")), "-home-u---");
}

#[test]
fn should_place_a_transcript_under_the_slugged_cwd() {
    let path = transcript_path(
        Path::new("/home/u/.claude/projects"),
        Path::new("/home/u/Projects/switchboard"),
        &SessionId::from("e0b4b21f"),
    );
    assert_eq!(
        path,
        PathBuf::from("/home/u/.claude/projects/-home-u-Projects-switchboard/e0b4b21f.jsonl")
    );
}

#[test]
fn should_borrow_the_workspace_name_from_the_path() {
    let cwd = PathBuf::from("/home/u/Projects/switchboard");
    let name: Option<&str> = workspace_name(&cwd);
    assert_eq!(name, Some("switchboard"));
    assert_eq!(workspace_name(Path::new("/")), None);
    assert_eq!(workspace_name(Path::new("relative")), Some("relative"));
}

#[cfg(unix)]
#[test]
fn should_return_none_when_the_last_component_is_not_utf8() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let cwd = Path::new("/home/u").join(OsStr::from_bytes(b"caf\xe9"));
    assert_eq!(workspace_name(&cwd), None);
    // Slugging still works, via the lossy form.
    assert_eq!(cwd_slug(&cwd), "-home-u-caf-");
}

#[test]
fn should_prefer_title_then_workspace_then_cwd_when_naming_an_agent() {
    // Built by the provided `discovered` constructor, not by the reducer,
    // so this test passes long before T07 exists.
    let t = common::Clock::new();
    let mut agent = switchboard::reduce::discovered(common::row(1, Some("s")), t.at(0));
    let a = &mut agent;

    a.title = Some("my-title".into());
    a.workspace = Some("ws".into());
    assert_eq!(display_name(a), "my-title");

    a.title = None;
    assert_eq!(display_name(a), "ws");

    a.workspace = None;
    assert_eq!(display_name(a), "p1");

    a.cwd = PathBuf::from("/");
    assert_eq!(display_name(a), "?");
}

#[test]
fn should_append_tmp_to_the_file_name_when_making_a_temp_sibling() {
    assert_eq!(
        temp_sibling(Path::new("/c/switchboard/state.json")),
        Some(PathBuf::from("/c/switchboard/state.json.tmp"))
    );
    assert_eq!(temp_sibling(Path::new("/")), None);
}
