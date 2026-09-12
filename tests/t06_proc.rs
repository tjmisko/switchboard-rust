//! T06 — finding agents in `/proc`.

mod common;

use std::borrow::Cow;
use std::path::Path;

use serde_json::json;

use common::FakeProc;
use switchboard::error::StatError;
use switchboard::model::{ProcKey, Provider};
use switchboard::proc::{
    ancestors, classify, key_of, parent_of, parse_cmdline, parse_stat, session_from_file, sweep,
};

const REAL: &str = "6002 (claude) S 5890 6002 5890 34818 6002 4194560 1311934 0 1085 0 43219 6390 0 0 20 0 19 0 12313 75436916736 125906 18446744073709551615 1 1 0 0 0 0 0 16781312 1024 0 0 0 17 5 0 0 0 0 0\n";

#[test]
fn should_parse_identity_parent_tty_and_cpu_from_a_real_stat_line() {
    let s = parse_stat(REAL).unwrap();
    assert_eq!(s.pid, 6002);
    assert_eq!(s.comm, "claude");
    assert_eq!(s.state, 'S');
    assert_eq!(s.ppid, 5890);
    assert_eq!(s.tty_nr, 34818);
    assert_eq!(s.cpu_ticks, 49609);
    assert_eq!(
        s.key(),
        ProcKey {
            pid: 6002,
            start: 12313
        }
    );
}

#[test]
fn should_split_at_the_last_paren_when_comm_contains_spaces_and_parens() {
    for comm in ["tmux: server", "a) (b", "))", "(", ""] {
        let line = REAL.replacen("(claude)", &format!("({comm})"), 1);
        let s = parse_stat(&line).unwrap_or_else(|e| panic!("{comm:?}: {e}"));
        assert_eq!(s.comm, comm);
        assert_eq!(s.start, 12313);
    }
}

#[test]
fn should_say_which_field_is_missing_or_malformed() {
    assert_eq!(parse_stat("6002 claude S 1"), Err(StatError::NoComm));
    assert_eq!(parse_stat("6002 )claude( S 1"), Err(StatError::NoComm));
    assert_eq!(
        parse_stat("6002 (claude)"),
        Err(StatError::Missing("state"))
    );
    assert_eq!(
        parse_stat("6002 (claude) S 5890 1 1 34818"),
        Err(StatError::Missing("utime"))
    );
    assert!(matches!(
        parse_stat(&REAL.replacen("5890", "ppid", 1)),
        Err(StatError::BadNumber { field: "ppid", .. })
    ));
    assert!(matches!(
        parse_stat(&REAL.replacen("6002", "x", 1)),
        Err(StatError::BadNumber { field: "pid", .. })
    ));
}

#[test]
fn should_split_cmdline_on_nul_and_borrow_when_valid_utf8() {
    let raw = b"claude\0--resume\0abc\0";
    let args = parse_cmdline(raw);
    assert_eq!(args, ["claude", "--resume", "abc"]);
    assert!(args.iter().all(|a| matches!(a, Cow::Borrowed(_))));
}

#[test]
fn should_repair_invalid_utf8_arguments_by_allocating() {
    let args = parse_cmdline(b"claude\0caf\xe9\0");
    assert_eq!(args[1], "caf\u{fffd}");
    assert!(matches!(args[1], Cow::Owned(_)));
    assert!(parse_cmdline(b"").is_empty());
    assert!(parse_cmdline(b"\0\0").is_empty());
}

#[test]
fn should_classify_an_interactive_claude_and_reject_everything_else() {
    let exe = Path::new("/home/u/.local/share/claude/versions/2.1.269");
    let yes = |argv: &[&str]| classify("claude", Some(exe), argv) == Some(Provider::Claude);

    assert!(yes(&["claude"]));
    assert!(yes(&["claude", "--resume", "abc"]));
    assert!(
        yes(&["claude", "--model", "daemon"]),
        "daemon only matters as the verb"
    );
    assert!(!yes(&["claude", "daemon"]));
    assert!(!yes(&["claude", "mcp", "serve"]));
    assert!(!yes(&["claude", "-p", "hi"]), "headless");
    assert!(
        !yes(&["claude", "--model", "x", "--print"]),
        "headless anywhere"
    );

    assert_eq!(classify("node", Some(exe), &["claude"]), None);
    assert_eq!(
        classify("claude", Some(Path::new("/usr/bin/python3")), &["claude"]),
        None
    );
    assert_eq!(
        classify("claude", None, &["claude"]),
        Some(Provider::Claude),
        "an unreadable exe is not a rejection"
    );
}

#[test]
fn should_accept_owned_and_borrowed_argv_alike() {
    let exe = Path::new("/x/claude/2");
    let owned: Vec<String> = vec!["claude".into()];
    let cows = parse_cmdline(b"claude\0");
    assert!(classify("claude", Some(exe), &owned).is_some());
    assert!(classify("claude", Some(exe), &cows).is_some());
}

#[test]
fn should_trust_a_session_file_only_when_pid_and_proc_start_match() {
    let key = ProcKey {
        pid: 6002,
        start: 12313,
    };
    let good = json!({"pid": 6002, "procStart": "12313", "sessionId": "e550f90b"});
    assert_eq!(session_from_file(&good, key).as_deref(), Some("e550f90b"));

    for bad in [
        json!({"pid": 6002, "procStart": "99", "sessionId": "e550f90b"}),
        json!({"pid": 7, "procStart": "12313", "sessionId": "e550f90b"}),
        json!({"pid": 6002, "procStart": 12313, "sessionId": "e550f90b"}),
        json!({"pid": 6002, "sessionId": "e550f90b"}),
        json!({"pid": 6002, "procStart": "12313", "sessionId": ""}),
        json!({"pid": 6002, "procStart": "12313"}),
    ] {
        assert_eq!(session_from_file(&bad, key), None, "{bad}");
    }
}

#[test]
fn should_sweep_a_fake_proc_for_live_interactive_claude_sessions_in_pid_order() {
    let root = common::scratch_dir("proc_sweep");
    let proc_root = root.join("proc");
    let sessions = root.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let fake = FakeProc { root: &proc_root };
    let cwd = Path::new("/home/u/Projects/switchboard");

    fake.add(300, "claude", 1, 900, &["claude"], cwd);
    fake.add(200, "claude", 1, 800, &["claude", "-p", "x"], cwd);
    fake.add(100, "claude", 1, 700, &["claude", "--resume"], cwd);
    fake.add(150, "bash", 1, 750, &["bash"], cwd);
    fake.add_zombie(400, "claude", 1);
    std::fs::create_dir_all(proc_root.join("self")).unwrap();
    std::fs::write(
        sessions.join("100.json"),
        r#"{"pid":100,"procStart":"700","sessionId":"sess-100"}"#,
    )
    .unwrap();
    std::fs::write(
        sessions.join("300.json"),
        r#"{"pid":300,"procStart":"1","sessionId":"stale"}"#,
    )
    .unwrap();

    let rows = sweep(&proc_root, &sessions);
    let pids: Vec<u32> = rows.iter().map(|r| r.key.pid).collect();
    assert_eq!(pids, [100, 300]);
    assert_eq!(rows[0].session.as_deref(), Some("sess-100"));
    assert_eq!(rows[1].session, None, "stale session file ignored");
    assert_eq!(rows[0].cwd, cwd);
    assert_eq!(rows[0].tty, Some(34816));
    assert_eq!(rows[0].cpu_ticks, 300);
}

#[test]
fn should_return_nothing_when_the_proc_root_does_not_exist() {
    assert!(sweep(Path::new("/definitely/not/here"), Path::new("/nor/here")).is_empty());
}

#[test]
fn should_walk_ancestors_until_the_root_or_the_limit() {
    let root = common::scratch_dir("proc_ancestors");
    let fake = FakeProc { root: &root };
    let cwd = Path::new("/");
    fake.add(1, "systemd", 0, 1, &["init"], cwd);
    fake.add(50, "claude", 1, 10, &["claude"], cwd);
    fake.add(60, "sh", 50, 20, &["sh"], cwd);
    fake.add(70, "switchboard-hoo", 60, 30, &["switchboard-hook"], cwd);

    assert_eq!(parent_of(&root, 70), Some(60));
    assert_eq!(parent_of(&root, 1), None, "ppid 0 is no parent");
    assert_eq!(parent_of(&root, 9999), None);

    let chain: Vec<u32> = ancestors(&root, 70, 20).collect();
    assert_eq!(chain, [70, 60, 50, 1]);
    let capped: Vec<u32> = ancestors(&root, 70, 2).collect();
    assert_eq!(capped, [70, 60]);
    let unknown: Vec<u32> = ancestors(&root, 9999, 20).collect();
    assert_eq!(unknown, [9999], "the starting pid is always yielded");

    assert_eq!(key_of(&root, 50), Some(ProcKey { pid: 50, start: 10 }));
}
