"""Regression cases for misleading joins found in the September captures."""

import contextlib
import io
import unittest
from collections import defaultdict

import analyze


def hook(event, ts, *, provider="codex", call=None, command="command-a", tool="Bash", **fields):
    return {"kind": "hook", "provider": provider, "event": event, "ts": ts,
            "command_hash": command, "input_hash": None, "agent": {"pid": 1},
            "payload": {"session_id": "session", "turn_id": "turn", "tool_name": tool,
                        "tool_use_id": call, **fields}}


def data(*hooks, sampler=()):
    return defaultdict(list, hooks=list(hooks), sampler=list(sampler))


class PermissionJoins(unittest.TestCase):
    def test_command_join_precedes_request_despite_different_input_hashes(self):
        pre = hook("PreToolUse", 10, call="call")
        ask = hook("PermissionRequest", 10.02)
        pre["input_hash"], ask["input_hash"] = "input", "input-with-description"
        unrelated = hook("PreToolUse", 30, call="other", command="other")
        self.assertIs(analyze.permission_pairs(data(pre, ask, unrelated), "codex")[0][1], pre)

    def test_missing_hashes_do_not_match(self):
        pre = hook("PreToolUse", 10, call="call", command=None)
        ask = hook("PermissionRequest", 10.02, command=None)
        self.assertIsNone(analyze.permission_pairs(data(pre, ask), "codex")[0][1])

    def test_other_session_child_turn_and_provider_do_not_match(self):
        ask = hook("PermissionRequest", 10)
        candidates = [hook("PreToolUse", 9, session_id="other"),
                      hook("PreToolUse", 9, agent_id="child"),
                      hook("PreToolUse", 9, turn_id="other"),
                      hook("PreToolUse", 9, provider="claude")]
        self.assertIsNone(analyze.permission_pairs(data(*candidates, ask), "codex")[0][1])

    def test_repeated_command_uses_nearest_call_once(self):
        old = hook("PreToolUse", 1, call="old")
        pre = hook("PreToolUse", 9.98, call="call")
        ask = hook("PermissionRequest", 10)
        pair = analyze.permission_pairs(data(old, pre, ask), "codex")[0]
        self.assertIs(pair[1], pre)
        self.assertEqual(pair[2], 2)
        pairs = analyze.permission_pairs(data(pre, ask, hook("PermissionRequest", 11)), "codex")
        self.assertIsNone(pairs[1][1])

    def test_only_same_call_completion_counts_even_after_five_hours(self):
        pre = hook("PreToolUse", 1, call="call")
        unrelated = hook("PostToolUse", 2, call="other")
        child = hook("PostToolUse", 3, call="call", agent_id="child")
        post = hook("PostToolUse", 18001, call="call")
        self.assertIs(analyze.tool_completion(data(unrelated, child, post), pre, 1), post)
        self.assertIsNone(analyze.tool_completion(data(unrelated, child), pre, 1))

    def test_request_and_question_pre_are_one_wait_not_two(self):
        pre = hook("PreToolUse", 1, provider="claude", tool="AskUserQuestion", call="call")
        ask = hook("PermissionRequest", 1.05, provider="claude", tool="AskUserQuestion")
        pre["input_hash"] = ask["input_hash"] = "question"
        unrelated = hook("PostToolUse", 2, provider="claude", call="background")
        post = hook("PostToolUse", 18001, provider="claude", tool="AskUserQuestion", call="call")
        sampler = [{"kind": "session_file", "pid": 1, "ts": ts,
                    "changed": {"status": [None, status]},
                    "file": {"sessionId": "session", "status": status, "waitingFor": None}}
                   for ts, status in [(1.1, "waiting"), (18001.1, "busy")]]
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            analyze.status_file_tracks_prompts(data(pre, ask, unrelated, post, sampler=sampler))
        self.assertIn("AskUserQuestion: 1/1", out.getvalue())
        self.assertIn("release vs matching tool completion (1 pairs)   p50 100 ms", out.getvalue())


if __name__ == "__main__":
    unittest.main()
