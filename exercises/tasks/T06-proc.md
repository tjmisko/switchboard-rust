# T06 — Finding agents in `/proc`

**Core: 45m** · Gaps: `ok_or` into `?` over many fields, `Cow<str>`,
`AsRef<str>` generics, `iter::successors`, `impl Iterator + '_` ·
Test: `cargo test --test t06_proc`

## Implement

**`src/proc.rs`**:

- `parse_stat(s: &str) -> Result<Stat, StatError>`. Split at the **last**
  `)`, because `comm` may contain `)` and spaces. Field N is `fields[N - 3]`.
  One small closure gives "field N or `Missing(name)`", and
  `number(text, name)` turns a `ParseIntError` into `BadNumber`.
- `parse_cmdline(bytes: &[u8]) -> Vec<Cow<'_, str>>`: split on NUL, drop
  empties, and use `String::from_utf8_lossy`. The test checks that valid
  arguments are `Cow::Borrowed`.
- `classify<S: AsRef<str>>(comm, exe: Option<&Path>, argv: &[S]) -> Option<Provider>`.
  The rules are in the doc comment. The generic is there so the function
  takes `&[String]`, `&[&str]` and `&[Cow<str>]` alike.
- `session_from_file(v: &Value, key: ProcKey) -> Option<SessionId>`.
  `procStart` is a **string**.
- `parent_of` and `ancestors(root, pid, limit) -> impl Iterator<Item = u32> + '_`:
  one `successors` call and one `take`.

`read_process` and `sweep` are provided. Read them: they are the I/O shell
around your parsers, and `exe.as_deref()` in `read_process` is T01's lesson
in one call.

## Katas

1. Why does `ancestors` need `+ '_` in its return type? What does it borrow?
2. `parse_cmdline` returns `Vec<Cow<'_, str>>`. If you'd written
   `Vec<String>`, which test would still pass, and what would it cost?
3. `classify(&stat.comm, exe.as_deref(), &argv)` where `exe: Option<PathBuf>`
   and `argv: Vec<Cow<str>>`. Give the type of each argument as the function
   receives it.

## Done when

`t06_proc` passes. That includes a sweep over a fake `/proc` built in the
target directory.
