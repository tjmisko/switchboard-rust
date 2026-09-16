# T05 — Finding agents in `/proc`

**Core: 55m** · Gaps: `ok_or` into `?` over many fields, `Cow<str>`,
`AsRef<str>` generics, `iter::successors`, `impl Iterator + '_`, an error's
`Display` and `source` written next to its raiser ·
Test: `cargo test --test t05_proc`

## Implement

**`src/error.rs`** first, and it is short: `Display` and `Error::source` for
`StatError`, the two bodies T02 left as `todo!`. `parse_stat` is the only
thing that raises this error, so write the messages for the person reading
a daemon log about a process that vanished mid-sweep. `t05_proc` pins them
only loosely: `starttime` must appear in the bad-number message, and the
`ParseIntError` inside `BadNumber` must come back from `source`, while
`NoComm` and `Missing` have none. The policy is the one in the `error.rs`
module comment: a variant that wraps another error names it in `Display`
*and* returns it from `source`, so a one-line `eprintln!("{e}")` says
everything and a reporter that walks the chain prints the cause twice, which
is the accepted price. Use the voice of the messages T02 pinned
(`stat field \`starttime\` is not a number: invalid digit found in string`
is golden's).

**`src/proc.rs`**:

- `parse_stat(s: &str) -> Result<Stat, StatError>`. Split at the **last**
  `)`, because `comm` may contain `)` and spaces. Field N is `fields[N - 3]`;
  the doc comment has the table of which fields go where (3 state, 4 ppid,
  7 tty_nr, 14 utime, 15 stime, 22 starttime) and which error names which
  field. One small closure gives "field N or `Missing(name)`", and
  `number(text, name)` turns a `ParseIntError` into `BadNumber`.
- `parse_cmdline(bytes: &[u8]) -> Vec<Cow<'_, str>>`: split on NUL, drop
  empties, and use `String::from_utf8_lossy`. The test checks that valid
  arguments are `Cow::Borrowed`.
- `classify<S: AsRef<str>>(comm, exe: Option<&Path>, argv: &[S]) -> Option<Provider>`.
  The four rules are in the doc comment, verb list included; the test is
  their examples. The generic is there so the function takes `&[String]`,
  `&[&str]` and `&[Cow<str>]` alike.
- `session_from_file(v: &Value, key: ProcKey) -> Option<SessionId>`.
  `procStart` is a **string**.
- `name_from_file(v, key) -> Option<Box<str>>`: the same file's `name`, the
  display name the operator recognises (`switchboard-rust-7f`), non-blank,
  under the same guard. Share the guard as `written_by(v, key) -> bool`. No
  captured hook carries a title, so this is where the chip's name comes from.
- `parent_of` and `ancestors(root, pid, limit) -> impl Iterator<Item = u32> + '_`:
  one `successors` call and one `take`. The chain begins with `pid`, so a
  `limit` of zero yields nothing.
- `key_of(root, pid) -> Option<ProcKey>`: read and parse the same `stat`
  file as `parent_of`, and hand back `Stat::key`. The two want a shared
  helper.

`read_process` and `sweep` are provided. Read them: they are the I/O shell
around your parsers, and `exe.as_deref()` in `read_process` is T01's lesson
in one call.

## Katas

1. Why does `ancestors` need `+ '_` in its return type? What does it borrow?
   Now remove `move` from the closure you passed to `successors`. What error
   do you get, and which `proc_root` is being borrowed: the parameter, or the
   `Path` it points at?
2. `parse_cmdline` returns `Vec<Cow<'_, str>>`. If you'd written
   `Vec<String>`, which test would still pass, and what would it cost?
3. `classify(&stat.comm, exe.as_deref(), &argv)` where `exe: Option<PathBuf>`
   and `argv: Vec<Cow<str>>`. Give the type of each argument as the function
   receives it.

## Done when

`t05_proc` passes. That includes a sweep over a fake `/proc` built in the
target directory.
