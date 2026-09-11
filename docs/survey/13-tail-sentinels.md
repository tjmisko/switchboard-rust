# Survey 13 — Tail sentinels: bad states the hooks never report

Date: 2026-09-11. Corpus: 2,376 transcript `.jsonl` files under
`~/.claude/projects` (1.1 GB, largest single file 20 MB, versions 2.1.2xx).
Producer: `~/.local/share/claude/versions/2.1.268`, read with `rg -a -o`.
Privacy: only structural fields, enum values and short sentinel strings are
quoted; no prompt text.

## 0. Findings in one screen

1. **There is exactly one transcript shape for "the API turn failed".** An
   `assistant` line with `"isApiErrorMessage":true`, an `error` enum, and the
   human text in `message.content[0].text`. Every `"error":` key in the corpus
   (165 lines) is on one of these lines; there is no other error line family.
2. **The usage-limit line carries a machine-readable reset time.**
   `quotaLimits.resetsAt` is epoch seconds, `quotaLimits.rateLimitType` is
   `five_hour` / `seven_day` / …; the prose `resets 4pm (America/Los_Angeles)`
   never needs parsing. Parse the prose only as a fallback.
3. **Stop does not fire on an API error; StopFailure does** (main thread
   only, and only if a StopFailure hook is registered). The Go daemon wires
   only Stop, so its chip stays "working" forever after a limit hit. That is
   the bug the user described.
4. **The sentinel is always within 4 KB of EOF while the session is stuck.**
   Measured over all 165 occurrences: the distance from the sentinel's line
   start to the next real activity (or EOF) is at most 3,464 bytes. A 16 KiB
   tail covers 100 % with 4x margin. The 128 KiB decode the Go daemon already
   does is more than enough; no new I/O is needed.
5. **Usage limits are account-wide.** A `five_hour` or `seven_day` rejection
   in one session blocks every session on the account until `resetsAt`. The
   daemon can hoist the deadline to a single `account.blocked_until` and
   colour every Claude root by it, including idle ones whose next prompt would
   fail.
6. **Auto-continue exists and is visible in the transcript.** A
   `system/informational` notice (`level:"notice"`) says
   `Usage limit reached · continuing automatically at 5:50pm · esc or type to cancel`;
   a matching `Usage limit reset · continuing automatically` line and a
   synthetic `user` prompt follow at reset time. Cancelling writes
   `Automatic continue cancelled · /rate-limit-options to re-arm`. So the same
   `resetsAt` is either "orange, will self-resume" or "orange, needs human".
7. **Retries never reach the transcript.** `Retrying in … attempt N/M` is a
   stream event only. By the time an `isApiErrorMessage` line is written the
   SDK's retry budget is exhausted; the line means the turn is over.

## 1. The sentinel catalogue

All counts are corpus-wide lines. "Class" is the proposed daemon
classification (§4). "Follows" is what the next non-bookkeeping line was.

| # | Sentinel (trimmed) | Where | `error` | `apiErrorStatus` | Count | Class | Reset time | Hook |
|---|---|---|---|---|---|---|---|---|
| S1 | `You've hit your session limit · resets 4pm (America/Los_Angeles)` | assistant, `isApiErrorMessage` | `rate_limit` | 429 | 75 | blocked-until-time | `quotaLimits.resetsAt` epoch; prose `resets <h[:mm]><am\|pm> (<IANA>)` | StopFailure(rate_limit) |
| S2 | `You've hit your weekly limit · resets Aug 25, 9pm (America/Los_Angeles)` | same | `rate_limit` | 429 | 16 | blocked-until-time | epoch; prose adds `<Mon> <d>, ` when not today | StopFailure |
| S3 | `You're out of usage credits. Run /usage-credits to keep using Fable 5.1 or /model to switch models.` | same | `rate_limit` | 429 | 44 | blocked-until-human | none: no `quotaLimits`; `errorDetails` holds the raw 429 body | StopFailure |
| S4 | `API Error: 529 Overloaded. This is a server-side issue, usually temporary — try again in a moment…` | same | `server_error` | 529 | 12 | blocked-until-human (transient) | none | StopFailure(server_error) |
| S5 | `Request timed out` | same | `server_error` | absent | 8 | blocked-until-human (transient) | none | StopFailure |
| S6 | `API Error: The response stopped arriving. The response above may be incomplete.` | same | `server_error` | absent | 3 | blocked-until-human (transient) | none | StopFailure |
| S7 | `API Error: Connection lost mid-response. The response above may be incomplete.` | same | `server_error` | absent | 2 | blocked-until-human (transient) | none | StopFailure |
| S8 | `Not logged in · Please run /login` / `Login expired · Please run /login` | same | `authentication_failed` | absent | 5 | blocked-until-human | none | StopFailure(authentication_failed) |
| S9 | `Usage limit reached · continuing automatically at 5:50pm · esc or type to cancel` | system, `subtype:"informational"`, `level:"notice"`, text in `content` | — | — | 4 | blocked-until-time, **self-resuming** | prose `at <h[:mm]><am\|pm>` (no zone; local) or `shortly`; pair with S1's epoch | Notification? (see §3) |
| S10 | `Usage limit reset · continuing automatically` | same | — | — | 3 | release edge (→ working) | — | Notification(quota_auto_resume_fired) |
| S11 | `Automatic continue cancelled · /rate-limit-options to re-arm` | same | — | — | 1 | blocked-until-human | — | Notification(quota_auto_resume_disabled)? |
| S12 | `[Request interrupted by user]` / `…for tool use]` | user text block | — | — | 164 / 29 | idle (already handled, survey 03 §5.2) | — | none |
| S13 | `"isAbortedMidStream":true` on an assistant line | assistant flag | — | — | 2 | idle (stream killed by Esc) | — | none |
| S14 | `Command timed out after 2m 0s` | user `tool_result`, `toolUseResult:"Error: Exit code 1\nCommand timed out…"` | — | — | 37 | **not a sentinel** (turn continues) | — | PostToolUseFailure |

Never observed here, though the binary has the strings: `Prompt is too long`
(the producer auto-compacts with trigger `ptl` and writes an API-error line
only if compaction is impossible), `overloaded` as an `error` value (529
lands as `server_error` with `apiErrorStatus:529`), and the rest of the
producer enum in §2.3.

Patterns from the brief that only match tool output or prose, never a key or a
`system` line, and must not be sentinels: `rate limit` (453 lines), `429`
(695), `500` (2,750), `context window` (190), `stream` (13,085), `timeout`
(11,976), `authentication` (1,392), `OAuth` (1,865), `expired` (635),
`retrying` (591), `ECONNRESET` (40), `fetch failed` (54).

## 2. Shapes

### 2.1 The API-error assistant line (S1–S8)

Invariants across all 165 lines (other keys are the usual envelope):

```json
{"type":"assistant","isApiErrorMessage":true,"error":"rate_limit","apiErrorStatus":429,
 "quotaLimits":{"status":"rejected","resetsAt":1788994800,"rateLimitType":"five_hour",
                "unifiedRateLimitFallbackAvailable":false,"overageStatus":"rejected",
                "overageDisabledReason":"out_of_credits","isUsingOverage":false},
 "timestamp":"2026-09-09T22:38:32.123Z",
 "message":{"role":"assistant","model":"<synthetic>","stop_reason":"stop_sequence",
            "usage":{"input_tokens":0,"output_tokens":0},
            "content":[{"type":"text","text":"You've hit your session limit · resets 4pm (America/Los_Angeles)"}]}}
```

- `message.model` is the literal `<synthetic>` (176 lines: the 165 errors plus
  11 `No response requested.` lines that have `isApiErrorMessage:false`).
- `message.stop_reason` is always `stop_sequence`; usage is all zeros.
- `error` ∈ {`rate_limit` 135, `server_error` 25, `authentication_failed` 5}.
- `apiErrorStatus` is present only when the HTTP status is known: 429 on
  every `rate_limit`, 529 on twelve of the `server_error`, absent on
  timeouts, dropped streams and auth failures.
- `errorDetails` is present on all 44 out-of-credits lines (S3) and on no
  other line; it is a string beginning
  `429 {"type":"error","error":{"type":"rate_limit_error","message":"This request would exceed your account's…`.
- `quotaLimits` is present on 71 of the 91 session/weekly-limit lines (S1/S2;
  absent on older 2.1.2xx builds) and never on S3. `overageDisabledReason:
  "out_of_credits"` appears on ordinary S1/S2 lines and says nothing about
  class. So: S3 ⇔ `rate_limit` with `errorDetails` and no `quotaLimits`, or
  text prefix `You're out of usage credits`.
- The line is 1.4–1.6 KB long (p50 1,463, max 1,583 bytes).
- `apiErrorIsTransient` exists in the producer but was never serialized.

### 2.2 The auto-continue notices (S9–S11)

```json
{"type":"system","subtype":"informational","level":"notice","isMeta":false,
 "content":"Usage limit reached · continuing automatically at 5:50pm · esc or type to cancel",
 "timestamp":"…","uuid":"…","parentUuid":"…","sessionId":"…","version":"2.1.2xx"}
```

`level` values in the corpus: `suggestion` (1,138, all `stop_hook_summary`),
`info` (219, `local_command`), `notice` (10), `warning` (7). Only `notice`
and `warning` carry limit or failure text. `system.subtype` values:
`stop_hook_summary`, `turn_duration`, `local_command`, `away_summary`,
`scheduled_task_fire`, `agents_killed`, `informational`, `bridge_status`.

The full observed sequence (3 of 4 armed cases resumed, 1 was cancelled):

```
assistant  isApiErrorMessage rate_limit   quotaLimits.resetsAt = T
system     informational/notice  "Usage limit reached · continuing automatically at 5:50pm · esc or type to cancel"  (+5 ms)
system     turn_duration                                                                             (+5 ms)
   … 40–52 min of nothing …
queue-operation ×2                                                                                   (T)
system     informational/notice  "Usage limit reset · continuing automatically"                       (T)
user       prompt (synthetic continuation)                                                            (T + 0.1 s)
assistant  … normal lines …                                                                           (T + 15–28 s)
```

Cancelled variant: `system informational/notice "Automatic continue cancelled · /rate-limit-options to re-arm"`
(+459 s), then nothing until a human prompt 40 min later.

### 2.3 Producer-side facts (2.1.268)

Constructor (minified `Ao`): `{content, apiError, apiErrorIsTransient, quotaLimits, error, errorDetails, truncatedAfterOutput}`
→ assistant message with `isApiErrorMessage:!0`, `stop_reason:"stop_sequence"`.

Status → `error` classifier:

```
529 or body contains "overloaded_error" → "overloaded"     (never seen serialized; 529 lands as server_error)
429                                     → "rate_limit"
401 | 403                               → "authentication_failed"
status >= 408                           → "server_error"
isCloudCredentialError                  → "cloud_credential_error"
else                                    → "unknown"
```

Full `error` enum (the StopFailure hook matcher list):
`rate_limit, overloaded, authentication_failed, oauth_org_not_allowed, account_on_hold, verification_required, billing_error, invalid_request, model_not_found, server_error, max_output_tokens, cloud_credential_error, unknown`.

Producer's own transience predicate: `isTransient(e) = e.apiErrorIsTransient === true || e.error === "overloaded" || e.error === "server_error"`.
Producer's own "hard limit" predicate: `apiErrorStatus === 429 && error === "rate_limit" && apiErrorIsTransient !== true`.

The producer's own session summariser maps every `error` except `unknown`
(→ `failed`) and `max_output_tokens` (→ none) to `{state:"blocked", needs}`.
§4 agrees and adds the time axis.

Limit names: `five_hour:"session limit", seven_day:"weekly limit", seven_day_opus:"Opus limit", seven_day_sonnet:"Sonnet limit", seven_day_overage_included:"Fable limit", overage:"usage credit limit"`.
Text template: `` `You've hit your ${limitName} · resets ${resetTime}${" · progress saved"?}` ``.
Auto-continue template: `"Usage limit reached · continuing automatically " + ("at <time>" | "shortly") + " · esc or type to cancel"`; the "again after you continued" variant appends `· the automatic-continue setting no longer ends this wait`.
Unobserved notice strings (all start with `Usage limit` or `Automatic continue`): `Usage limit available again · continuing now`, `Usage limit has reset · press enter to continue`, `Automatic continue was turned off…`, `Automatic continue stopped…`, `Automatic continue did not run…`.

Retry: the SDK retries with `maxRetries` (honouring `retry-after`), emits
`{kind:"retrying", attempt, maxRetries, retryInMs}` stream events only, and
writes the error line after `api_request_retry_exhausted`. Corpus: 0
`agent_api_retry` lines, 0 `Retrying in` text values.

## 3. What happens next, and which hooks fire

Observed follow-up for the 165 error lines (bookkeeping lines skipped):

| `error` | still EOF | human prompt | local `/`cmd | assistant resumed | tool_result |
|---|---|---|---|---|---|
| `rate_limit` (135) | 119 | 12 | 4 | 0 | 0 |
| `server_error` (25) | 19 | 3 | 1 | 1 | 1 |
| `authentication_failed` (5) | 0 | 4 | 0 | 1 | 0 |

Every class stops the turn. No `server_error` recovered by itself; the one
`assistant resumed` case is a subagent its parent re-drove. The producer's
"transient" label means "safe to retry", not "will retry". **No sentinel is
transient in the auto-recover sense.** The only self-resuming path is
auto-continue (S9 → S10), and it is explicit in the transcript.

Hook coverage, from the emitter in the binary (`FQe`):

- **StopFailure** fires on every `isApiErrorMessage` line (S1–S8), on
  `prompt_too_long` when compaction is impossible, on compaction API failure
  and on `malformed_tool_use_exhausted`. Payload
  `{hook_event_name:"StopFailure", error, error_details?, last_assistant_message?}`,
  matcher = `error`. Main thread only (any agent context returns early), and
  only if a StopFailure hook is registered. The daemon must register one.
- **Stop** does not fire on any of these paths: the query loop returns
  `{reason:"api_error"}` before the stop-hook runner. Corpus confirmation:
  the 17 sentinel files contain 240 `stop_hook_summary` lines for normal
  turns and zero within the bookkeeping tail of any sentinel.
- **Notification** types `quota_auto_resume_fired` / `_stale` / `_disabled`
  fire on the `fired-now`, `stale`, `disabled`, `horizon-exceeded`,
  `continuation-dropped` and `cap-exhausted` branches. The `auto-armed` and
  `cancelled` branches write only the transcript notice (S9, S11). Untested
  live; the transcript line is the reliable signal either way.
- **PostToolUseFailure** covers S14; S12/S13 fire nothing (known, survey 03
  §8).

With StopFailure wired the daemon learns "turn failed with `error`" at the
same instant as the line. The tail scan is still needed for subagent and
teammate roots (no StopFailure), for hydrate, for `resetsAt` and the
auto-continue facts StopFailure does not carry, and for the account hoist.

## 4. Classification

| Class | Members | Chip | Leaves when |
|---|---|---|---|
| `blocked_until(T)` self-resuming | S1/S2 followed by S9 within the same tail, `T = quotaLimits.resetsAt` (or S9's `at <time>` today in local zone) | orange, shows `⏱ 4:00pm` | S10 line or any later assistant/user line, or `now ≥ T + 60 s` grace |
| `blocked_until(T)` needs human | S1/S2 without S9, or S11 after S9 | orange, `⏱ 4:00pm` | later user/assistant line; at `T + 60 s` becomes **idle** with a "limit reset" note (the next prompt will work) |
| `blocked(human)` | S3 (out of credits), S4–S7 (server), S8 (auth) | orange, reason text | any later user/assistant line |
| idle | S12, S13 | as today | — |
| not a sentinel | S14 | — | — |

The AP8 asymmetry decides the ties: a false "blocked" costs one glance and
is corrected by the next line the tailer sees; a missed one is a session
that looks green for an hour. So: any `isApiErrorMessage:true` line that is
the last non-bookkeeping line is blocked, regardless of `error`, including
`unknown`. `server_error` is orange, not green-with-a-cap, because the
corpus shows zero self-recoveries.

Reset time parse rules, in order of trust:

1. `quotaLimits.resetsAt` (epoch seconds). Use if present and
   `rateLimitType ∈ {five_hour, seven_day, seven_day_opus, seven_day_sonnet}`.
   A `rate_limit` line with no `quotaLimits` and an `errorDetails` string, or
   text starting `You're out of usage credits`, is S3: no clock, `blocked(human)`.
2. Text `resets <Mon> <d>, <h>[:<mm>]<am|pm> (<IANA>)` or
   `resets <h>[:<mm>]<am|pm> (<IANA>)`. Interpret in the named zone; a
   time-only form is "next occurrence at or after the line's `timestamp`".
   Seen forms: `4pm`, `8:20pm`, `10:50pm`, `Aug 25, 9pm`.
3. S9 text `continuing automatically at <h>[:<mm>]<am|pm>` (no zone; the
   process's local zone) or `shortly`. Only needed if 1 and 2 are absent.

## 5. Scanner design

**Trigger, not tick.** The rewrite's transcript tailer (plan §3, survey 03
§8 "one decoded tail per file per tick, cached by (path, size, mtime)")
already yields new complete lines. The sentinel check is a fold over those
lines, so its steady-state cost is zero when the file is unchanged. The
byte-level substring pre-filter below matters for hydrate and for the
periodic backstop re-read, where the tail is scanned cold.

**Window.** 16 KiB is 4x the largest observed blocked region (3,464 bytes);
use 64 KiB for margin against a fat `errorDetails`. One `pread` at
`size - 64 KiB`, drop bytes up to the first `\n`, ignore a trailing partial
line; records are single-line JSON. Never scan the whole file: a tool result
line can be 1.35 MB, and a sentinel 12 MB from EOF is a session that moved
on hours ago.

**Pre-filter.** Literal substrings, not regex, one Aho-Corasick pass (or
four `memmem` calls) over the window: `"isApiErrorMessage":true`,
`"subtype":"informational"`, `[Request interrupted by user`,
`"isAbortedMidStream":true`. Inside an `informational` line's `content`
only: `Usage limit reached`, `Usage limit reset`, `Usage limit available`,
`Automatic continue`. Everything else in the brief is noise. JSON-decode only
the lines that hit, with a narrow borrowed struct; a 1 MB tool result is the
common case in the window and must not be parsed.

**Walk backward.** The fact is "what is the last thing the model did".
Iterate from the end, skipping bookkeeping types (`last-prompt`,
`queue-operation`, `file-history-*`, `custom-title`, `ai-title`,
`agent-name`, `mode`, `permission-mode`, `atis-latch`, `cost-state`,
`bridge-session`, `attachment`, `system/turn_duration`,
`system/stop_hook_summary`). The first other line decides: an
`isApiErrorMessage` assistant line → blocked (class by §4), self-resuming
if an S9 notice sits after it; S9 alone → blocked_until with `T` from the
preceding S1; S10 → release edge (working; the synthetic prompt follows
within 0.1 s); S11 → blocked(human); S12/S13 → idle; anything else → no
sentinel, the hook state stands.

**State gating.** Scan on `working`/`delegating` (the false-green case), at
hydrate, and on `idle` too: a limit that will block the *next* prompt earns
an orange tint, and the account hoist needs every root's latest `resetsAt`.
Do not scan subagent transcripts for this; a child's limit hit becomes the
parent's S1 within seconds, and the fanout reducer already marks the child
errored from its last line.

**Expiry.** `blocked_until(T)` is `Effect::Deadline{purpose: LimitReset}`;
at `T + 60 s` with no new lines the root goes idle with a "limit reset"
note, never back to working. `blocked(human)` has no clock; it leaves only
on a later line or a `UserPromptSubmit`.

## 6. Reducer rule and graph integration

Not a new `Attention` value (attention is per-writer prompt ownership, plan
§5 rule 4) and not a `Runtime` value (`SystemError` already maps to grey).
Add a third axis on the root:

```rust
pub enum Block {
    None,
    UntilTime { at: Timestamp, resumes: bool, kind: LimitKind },   // orange + clock
    Human     { reason: BlockReason },                              // orange + reason
}
pub struct ClaudeRoot { …, block: Block, block_seen_at: Option<Timestamp> }
```

Fold order (survey 03 §2.2, extended): not fresh → grey; waiting > 0 →
permission (red beats orange: a pending approval is still the most
actionable thing); **block != None → blocked (orange)**; root active →
working; root system_error → grey; live child active → delegating; root
idle → idle; else grey.

Wire projection: `status: "blocked"`, `blocked_until: Option<RFC3339>`,
`block_reason: Option<String>`, `block_resumes: bool`; the renderer shows
`⏱ 4:00pm` in the user's zone when the clock is set, else the reason.
`since` is the sentinel's own `timestamp`, floored at the previous anchor
(plan §5 rule 6).

Account hoist: the core keeps `max(resetsAt)` over all Claude roots with a
`five_hour`/`seven_day` block whose `resetsAt > now`; every Claude root that
is `idle` gets `block = UntilTime{…, resumes:false}` from it. This is
derived at publish time, not stored per root, so it clears on its own.

Release edges, in priority: a later non-bookkeeping line from the same
writer; `UserPromptSubmit`; S10; deadline `T + 60 s` (`UntilTime` only).
Hooks never *set* a block: StopFailure is a same-instant confirmation, so
map it to `Invalidate(root)` and let the tailer re-read now. At hydrate the
scan runs on every Claude root; a persisted `Block` is falsified against the
tail, never manufactured (rule 11 analogue).

## 7. Cost

Measured on the 20 MB transcript (aarch64, warm page cache):

| Operation | median | p95 |
|---|---|---|
| `tail -c 262144 \| rg -c -F` (6 literals), two processes | 2.3 ms | 3.7 ms |
| `tail -c 65536 \| rg -c -F` | 2.5 ms | 4.7 ms |
| `rg -c -F` over the whole 20 MB file | 6.2 ms | 13.6 ms |
| in-process: `pread` 64 KiB + 6 substring searches (Python) | 0.17 ms | — |
| in-process: `pread` 256 KiB + 6 substring searches (Python) | 0.35 ms | — |

Process spawn dominates the shell numbers. In Rust the 64 KiB read plus an
Aho-Corasick pass is well under 100 µs; five sessions per tick is under
1 ms cold and zero when `(size, mtime)` is unchanged. Do not shell out.

## 8. Rust sketch

```rust
// crates/switchboard/src/claude/sentinel.rs

pub const TAIL_WINDOW: u64 = 64 * 1024;          // 4x+ the largest observed blocked region (3,464 B)
pub const RESET_GRACE: Duration = Duration::from_secs(60);

/// Byte needles for the pre-filter. Order is irrelevant; all are checked.
pub const NEEDLES: &[&[u8]] = &[
    br#""isApiErrorMessage":true"#,
    br#""subtype":"informational""#,
    b"[Request interrupted by user",
    br#""isAbortedMidStream":true"#,
];

/// Substrings that make an `informational` line a sentinel. Checked against `content` only.
pub const NOTICE_NEEDLES: &[(&str, Notice)] = &[
    ("Usage limit reached", Notice::AutoContinueArmed),
    ("Usage limit reset", Notice::AutoContinueFired), ("Usage limit available again", Notice::AutoContinueFired),
    ("Automatic continue cancelled", Notice::AutoContinueOff), ("Automatic continue was turned off", Notice::AutoContinueOff),
    ("Automatic continue stopped", Notice::AutoContinueOff), ("Automatic continue did not run", Notice::AutoContinueOff),
    ("Usage limit has reset", Notice::StaleNeedsEnter),
];

/// Line types that carry no model activity; skipped when walking backward.
pub const BOOKKEEPING: &[&str] = &["last-prompt","queue-operation","file-history-snapshot",
    "file-history-delta","custom-title","ai-title","agent-name","mode","permission-mode",
    "atis-latch","cost-state","bridge-session","attachment","pr-link","frame-link"];
pub const BOOKKEEPING_SYSTEM: &[&str] = &["turn_duration","stop_hook_summary","away_summary"];

pub enum Sentinel {
    ApiError { error: ApiErrorKind, status: Option<u16>, text: String, resets_at: Option<Timestamp>,
               limit: Option<LimitKind>, out_of_credits: bool /* errorDetails && !quotaLimits */, at: Timestamp },
    Notice { kind: Notice, at: Timestamp, at_text: Option<String> },   // "at 5:50pm" | "shortly"
    Interrupted { at: Timestamp }, AbortedMidStream { at: Timestamp },
}
// Narrow borrowed serde view (unknown fields ignored): type, subtype, level, content, isApiErrorMessage,
// error, apiErrorStatus, quotaLimits{resetsAt, rateLimitType}, isAbortedMidStream, timestamp, message.content[0].text.

/// Read the last `TAIL_WINDOW` bytes, aligned to a line start, dropping a trailing partial line.
pub fn read_tail(path: &Path) -> io::Result<Vec<u8>> { /* pread at max(0, len - TAIL_WINDOW); trim to first '\n' */ }

/// The last non-bookkeeping line, classified. `None` = no sentinel, hook state stands.
pub fn scan(tail: &[u8]) -> Option<Sentinel> {
    if !NEEDLES.iter().any(|n| memchr::memmem::find(tail, n).is_some()) { return None; }  // cheap exit
    let mut armed_after: Option<Sentinel> = None;
    for line in tail.rsplit(|b| *b == b'\n').filter(|l| !l.is_empty()) {
        let Ok(l) = serde_json::from_slice::<Line>(line) else { continue };      // corrupt line = skip
        if BOOKKEEPING.contains(&l.ty) { continue; }
        if l.ty == "system" && l.subtype.map_or(true, |s| BOOKKEEPING_SYSTEM.contains(&s)) { continue; }
        match classify(&l) {
            Some(s @ Sentinel::Notice { .. }) => { armed_after.get_or_insert(s); continue; } // keep walking to the S1 that owns the clock
            Some(s) => return Some(merge(s, armed_after)),
            None => return armed_after,   // a real line: the notice, if any, is the newest fact
        }
    }
    armed_after
}

/// `resets 4pm (America/Los_Angeles)` | `resets Aug 25, 9pm (America/Los_Angeles)` → Timestamp (jiff).
/// Split on "resets ", rsplit on " (" for the zone, optional "<Mon> <d>, ", then "<h>[:<mm>]<am|pm>";
/// a date-less time earlier than `line_ts` means tomorrow. Fallback only; the epoch wins when present.
pub fn parse_reset_text(text: &str, line_ts: Timestamp) -> Option<Timestamp> { todo!() }

pub fn to_block(s: &Sentinel, now: Timestamp) -> Block {
    match s {
        Sentinel::ApiError { error: ApiErrorKind::RateLimit, resets_at: Some(t), out_of_credits: false, limit, .. }
            if *t > now => Block::UntilTime { at: *t, resumes: false, kind: limit.unwrap_or(LimitKind::Unknown) },
        Sentinel::ApiError { .. } => Block::Human { reason: reason_of(s) },
        Sentinel::Notice { kind: Notice::AutoContinueFired, .. } => Block::None,
        Sentinel::Notice { .. } => unreachable!("merged into the owning ApiError by scan()"),
        Sentinel::Interrupted { .. } | Sentinel::AbortedMidStream { .. } => Block::None,   // idle evidence for the runtime axis
    }
}
```

`merge(s, armed_after)` sets `resumes = true` when the notice is
`AutoContinueArmed` and `false` when it is `AutoContinueOff`; `StaleNeedsEnter`
is `Human { reason: "press enter" }`.

### Tests to write (fixtures are the lines quoted above with ids redacted)

- `should_classify_session_limit_as_until_time_when_resets_at_is_future`
- `should_prefer_quota_epoch_over_prose_when_both_present`
- `should_parse_prose_reset_when_quota_limits_absent` (older builds; 20 lines), covering the
  time-only form rolling to tomorrow and the `Aug 25, 9pm (America/Los_Angeles)` form
- `should_classify_out_of_credits_as_human_when_quota_limits_absent_and_error_details_present`
- `should_classify_server_error_and_auth_failure_as_human` (529, timed out, stopped arriving, connection lost, /login)
- `should_mark_resumes_true_when_auto_continue_notice_follows_limit`
- `should_mark_resumes_false_when_cancel_notice_follows_armed_notice`
- `should_clear_block_when_reset_notice_is_newest_line`
- `should_skip_bookkeeping_lines_when_walking_back` (turn_duration, last-prompt, queue-operation ×2, file-history-snapshot)
- `should_return_none_when_a_tool_result_mentions_usage_limit_in_prose`
- `should_return_none_when_newest_line_is_a_normal_assistant_line_after_an_old_error`
- `should_ignore_partial_first_and_last_lines_when_window_cuts_a_record`
- `should_not_decode_lines_that_miss_every_needle` (a 1 MB tool-result line in the window; assert no allocation beyond the tail buffer)
- Reducer: `should_expire_until_time_block_to_idle_after_grace`,
  `should_keep_permission_above_blocked_in_fold`,
  `should_hoist_account_block_to_idle_roots_when_any_root_has_future_reset`,
  `should_release_block_on_user_prompt_submit_hook`.
- Live checks, not unit tests: does `Notification{quota_auto_resume_fired}`
  fire on 2.1.268; does `StopFailure` fire for a `-p` headless root; the S9
  `at <time>` zone when `TZ` differs from the IANA zone in the S1 text.

## 9. Open questions

- S1's zone is the account's (`America/Los_Angeles` here); S9's `at 5:50pm`
  is process-local. They coincide on this machine; do not assume it.
- `seven_day_overage_included` and `overage` kinds produce no `resetTime` in
  the producer; if they ever serialize, class them `Human`.
- Whether a `-p` headless root should show `blocked` at all; the shape is
  identical but the process usually exits shortly after.
