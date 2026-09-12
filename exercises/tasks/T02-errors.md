# T02 — Error architecture

**Core: 1h15** · Gaps: hand-written errors, `Display` / `Error::source`,
`From` and `?`, `ok_or` vs `ok_or_else`, `transpose`, `Box<dyn Error>` ·
Tests: `cargo test --test t02_errors`, and **`t02_forward`, which you write**

Read the module comment at the top of `src/error.rs` before anything else. The
rule that shapes every type in the file is **absence is not failure**. A
missing optional field is `None` and travels as data. It turns into an error
only at the point where someone requires it, and then the error names the
field.

## Implement

**`src/error.rs`**. The enums are given. Write:

- `Display` for `UnknownName`, `FrameError`, `StatError`, `RecordError` and
  `ForwardError`. The test file pins the exact wording of several messages.
- `Error::source` wherever a variant wraps another error.
- `From<serde_json::Error>` for `FrameError`, `RecordError` and
  `ForwardError`; `From<io::Error>` and `From<UnknownName>` for
  `RecordError`.
- **Not** `From<io::Error>` for `ForwardError`. The doc comment says why. Make
  sure you agree before moving on.

**`src/names.rs`**: `FromStr` for `Phase` (use `Phase::ALL`, no second list of
names), `StartSource` and `Denier`. All return `UnknownName`.

**`src/ingest/frame.rs`**:

- `decode_request(line: &str) -> Result<Request, FrameError>`
- `required_str` and `optional_pid`. `optional_pid` is the one to think
  about. A missing `pid` is fine; a `"4036"`, `-1`, `1.5` or `2^32` is an
  error. Write it as `v.get("pid").map(...).transpose()`, and write the type
  of the expression after each step in a comment.

**`src/ingest/forward.rs`**: `interpret_reply(line: &str) -> Result<(), ForwardError>`.

**`src/client.rs`**: `send(socket, frame, timeout) -> Result<(), ForwardError>`.
Every I/O step maps its error to the variant naming that step.

## You write the tests: `tests/t02_forward.rs`

The file lists the test names and says what each must establish. Write the
bodies **before** `interpret_reply` and `send`, and watch them fail
(`todo!` panics count as failing). For `send` you need something to talk to.
A `std::os::unix::net::UnixListener` on a thread that reads one line and writes
a canned reply is enough, and a second variant that never replies covers the
timeout. Keep socket paths short: `SUN_LEN` is 108 bytes, and the cargo
target dir is long.

The four failure classes are the same ones you struggled to keep apart in the
HTTP client: transport, remote refusal, malformed response, and absent field.
The tests should make each one impossible to confuse with the others.

## Katas

1. `ok_or(FrameError::Missing("cmd"))` vs `ok_or_else(|| FrameError::Missing("cmd"))`.
   Which is right here, and when would the other one matter?
2. Give the type at each step:
   `v.get("pid")` → `.map(|raw| raw.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or(E))` → `.transpose()`
3. `fn main() -> Result<(), BoxError>` can `?` a `FrameError`, a
   `StatError` and an `io::Error` in the same body. Which trait impls make
   that work, and which of them did you write?

## Done when

`t02_errors` and your `t02_forward` pass. After grading, the hidden
`t02_forward_golden` passes too.
