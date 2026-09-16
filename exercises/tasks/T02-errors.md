# T02 — Error architecture

**Core: 1h15** · Gaps: hand-written errors, `Display` / `Error::source`,
`From` and `?`, `ok_or` vs `ok_or_else`, `transpose`, `Box<dyn Error>` ·
Tests: `cargo test --test t02_errors`, and **`t02_forward`, which you write**

Read the module comment at the top of `src/error.rs` before anything else. The
rule that shapes every type in the file is **absence is not failure**. A
missing optional field is `None` and travels as data. It turns into an error
only at the point where someone requires it, and then the error names the
field. The same comment states the `Display`-versus-`source` policy: a
variant that wraps an error names it in `Display` *and* returns it from
`source`. Follow that in every enum here; the tests pin both halves.

## Implement

**`src/error.rs`**. The enums are given. Write:

- `Display` for `UnknownName`, `FrameError`, `StatError`, `RecordError` and
  `ForwardError`. `t02_errors` pins the exact wording of the `UnknownName`
  and `FrameError` messages and of `ok_reply` / `error_reply`. `StatError`
  and `RecordError` are pinned only loosely (`starttime` must appear in the
  bad-number message); choose wording in the same voice as the pinned ones.
  For `ForwardError`, one message is pinned by the hidden suite: a refusal
  displays as `daemon refused the frame: <reason>`, exactly that prefix.
- `Error::source` wherever a variant wraps another error.
- `From<serde_json::Error>` for `FrameError`, `RecordError` and
  `ForwardError`; `From<io::Error>` and `From<UnknownName>` for
  `RecordError`.
- **Not** `From<io::Error>` for `ForwardError`. The doc comment says why. Make
  sure you agree before moving on.

**`src/names.rs`**: `FromStr` for `Phase` (use `Phase::ALL`, no second list of
names), `StartSource` and `Denier`. All return `UnknownName`. The wire
vocabularies are in the doc comments on the two enums in `model.rs`:
`StartSource` is `startup`, `resume`, `clear`, `compact`; `Denier` is the
transcript's `toolDenialKind` set. `Origin::from_kind` and
`ErrorClass::from_wire` are provided; read them to see what a *total* parser
looks like next to your fallible ones.

**`src/ingest/frame.rs`**:

- `decode_request(line: &str) -> Result<Request, FrameError>`
- `required_str` and `optional_pid`. `optional_pid` is the one to think
  about. A missing `pid` is fine; a `"4036"`, `-1`, `1.5` or `2^32` is an
  error. Write it as `v.get("pid").map(...).transpose()`, and write the type
  of the expression after each step in a comment.

**`src/ingest/forward.rs`**: `interpret_reply(line: &str) -> Result<(), ForwardError>`.
Two things the hidden suite pins: `{"ok":false}` with no `error`, or with an
`error` that is not a string, is `Rejected` with the reason
`no reason given`, verbatim; and a boolean `ok` is the only verdict, so
`{"ok":"true"}` is `NoVerdict`, not success.

**`src/client.rs`**: `send(socket, frame, timeout) -> Result<(), ForwardError>`.
Every I/O step maps its error to the variant naming that step.

- A `UnixStream` has no timeouts until you install them. Call
  `set_read_timeout` (and `set_write_timeout`) with `timeout`, or a daemon
  that accepts and says nothing blocks the forwarder forever. Nothing in
  `std` does this for you.
- `Value` implements `Display`, so `frame.to_string()` is the wire form and
  cannot fail. Do not reach for `serde_json::to_vec(frame)?`: it compiles,
  and the `?` routes a serialisation failure through
  `From<serde_json::Error>` into `ForwardError::Malformed`, whose doc says
  "the reply is not JSON". A wrong variant that compiles is the trap.
- `Read` and `Write` are implemented for `&UnixStream`, so one stream can be
  written through one shared borrow and read through a `BufReader::new(&stream)`
  without moving it.
- A daemon that hangs up before replying gives `read_line` an `Ok(0)` and an
  empty line. Golden hands that empty line to `interpret_reply`, which calls
  it `Malformed`; treating it as `Io` instead is defensible, and the hidden
  suite does not test it. Say which you chose in your own test.

## You write the tests: `tests/t02_forward.rs`

The file lists the test names and says what each must establish. Write the
bodies **before** `interpret_reply` and `send`, and watch them fail
(`todo!` panics count as failing). The helpers `socket` and `one_shot` are
provided at the top of the file: read them, and notice what `None` does.
The silent daemon holds the connection open for five seconds, far longer
than any timeout you pass, so the timeout test must also assert that `send`
returned *well before* that. Without the elapsed-time check, a `send` with
no read timeout passes by waiting for the daemon to hang up. Keep socket
paths short: `SUN_LEN` is 108 bytes, and the cargo target dir is long.

The given `t02_errors` uses `let ... else`. If that is new: it is an
`if let` whose `else` branch must diverge (`return`, `panic!`, `continue`),
so the bound names are in scope for the rest of the block.

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
`t02_forward_golden` passes too. Everything it pins beyond your own tests is
named on this sheet: the `no reason given` reason, the
`daemon refused the frame: ` prefix, and a timeout that fires before the
silent daemon gives up.
