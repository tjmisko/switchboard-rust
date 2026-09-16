# T01 — Identifiers and borrowed strings

**Core: 1h** · Gaps: Deref coercion, `as_ref` / `as_deref`, `Borrow`,
`&str` vs `String`, `&Path` vs `PathBuf` · Test: `cargo test --test t01_names`

Every id in the model is `struct SessionId(pub Box<str>)`. That's the right
type, and unpleasant to use until it has the conversions that let it pass
for a `&str` wherever a string is only being read. This task adds those
conversions, then uses them on paths.

## Implement

**`src/names.rs`**, the bodies inside `id_impls!`. The impl blocks are already
there with `todo!()` bodies, because the rest of the crate won't compile
without the signatures. One macro arm generates, for every id type:

- `as_str(&self) -> &str`
- `Deref<Target = str>`
- `AsRef<str>`
- `Borrow<str>`
- `Display`, which writes the bare id
- `From<&str>` and `From<String>`. The `String` one must reuse the
  allocation: `String::into_boxed_str` is the method. It calls
  `shrink_to_fit` first, so it copies only when the `String` had spare
  capacity. `Box::from(&str)` always copies, which is fine for the `&str`
  impl and wrong for this one.

Inside the macro, `$name` is the type and `$name(x)` constructs one. If macro
syntax gets in the way, write the `SessionId` versions in a scratch module
first, then paste the bodies back.

Also in `names.rs`:

- `SubagentId::from_file_stem(stem: &str) -> Option<SubagentId>`: strip
  `agent-` **exactly once**; `None` if the prefix is absent or nothing remains.
- `SubagentId::from_hook(raw: &str) -> Option<SubagentId>`: trim; `None` if
  blank.

**`src/paths.rs`**:

- `cwd_slug(cwd: &Path) -> String`: every **character** that is not ASCII
  alphanumeric becomes `-`. Character, not byte: `é` is two bytes and one
  `char`, and it becomes one `-`. A path need not be UTF-8, so start from
  `to_string_lossy()`, which returns a `Cow<str>`; the otherwise unused
  `use std::borrow::Cow` at the top of the file is there so you can write
  the type out.
- `transcript_path(projects: &Path, cwd: &Path, session: &SessionId) -> PathBuf`.
  `format!("{session}.jsonl")` compiles only because of the `Display` you
  wrote above.
- `workspace_name(cwd: &Path) -> Option<&str>`: note the lifetime; it borrows
  from `cwd`.
- `display_name(agent: &Agent) -> &str`: title, else workspace, else the last
  cwd component, else `"?"`. **One expression, no `match`.** Its test builds
  the `Agent` with the provided `switchboard::reduce::discovered`, so it
  passes long before the reducer exists.
- `temp_sibling(path: &Path) -> Option<PathBuf>`: `state.json` becomes
  `state.json.tmp`. Build the name as an `OsString`, not a `String`.

## Things to notice while you do it

- `Borrow<str>` is what makes `map.get("a1")` compile on a
  `BTreeMap<SubagentId, _>`. It has a contract that `AsRef` doesn't: `Eq`,
  `Ord` and `Hash` must agree between the owned and borrowed forms. Why does
  a derived impl on a one-field tuple struct satisfy it?
- The live reason the impl exists is `gate_of` in `tests/common/mod.rs`,
  which calls `tools.get(call)` with a `&str` on a map keyed by `ToolUseId`.
  Every test file compiles `common`, so comment the `Borrow` impl out and
  read the error before you write it.
- `title: Option<Box<str>>`. `.as_ref()` gives `Option<&Box<str>>`;
  `.as_deref()` gives `Option<&str>`. `display_name` needs the second.
- `SubagentId::from_file_stem("agent-").as_deref()` works in the tests only
  because of *your* `Deref` impl.

## Katas (answer in `exercises/answers/T01.md`)

1. Why doesn't this compile, and what are the two different fixes?
   ```rust
   fn first(ids: Vec<SessionId>) -> &str { ids[0].as_str() }
   ```
2. In `fn f(names: &[&str])`, what is the type of `n` in
   `names.iter().filter(|n| n.len() > 3)`? Why does `n.len()` still work?
3. `impl Deref for SessionId` lets `&SessionId` coerce to `&str`. Why does
   *not* also let a `SessionId` be passed where a `ToolUseId` is expected?
   Which property of the newtype survives, and which is weakened?

## Done when

`t01_names` passes, and so do the unit tests in `names.rs` and `paths.rs`:
`cargo test --lib -- names:: paths::`. Nothing in either module's tests
needs a later task.
