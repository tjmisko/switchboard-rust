# Scaffold notes for T14

What the scaffold (`exercises`, then `main`) needs from this branch, exactly.
Everything outside function bodies mirrors golden verbatim; the bodies are
listed below. The golden commits are `feat(rpc)`, `feat(watcher)`,
`test(T14)`, `docs(T14)` and this file's `docs(T14): scaffold notes`.

## `src/daemon/watcher.rs`

- Imports: golden's line, `use tokio::sync::{mpsc, watch};` (was `mpsc` only).
- `run`: body stays `todo!("T13")`. Doc comment becomes golden's new one
  (delegation to `run_until`; why the sender is held). The T14 sheet tells
  the learner to move their T13 loop into `run_until` and make `run`
  delegate.
- `run_until`: new, signature and doc comment from golden, body
  `todo!("T14")`. Keep the parameter spelled `mut stop` as golden has it;
  the resulting `unused_mut` is hidden by `lib.rs`'s `#![allow(unused)]`
  like every other stub's unused parameters.

```rust
pub async fn run_until(
    spec: WatchSpec,
    inbox: mpsc::Sender<Input>,
    mut stop: watch::Receiver<bool>,
) {
    todo!("T14")
}
```

- `poll_once`, `open_at_tail`: unchanged, `todo!("T13")`.

## `src/daemon/rpc.rs`

- Imports: golden's `tokio::io` line, which adds `AsyncBufRead` and `Lines`.
  Unused in the scaffold, hidden by `#![allow(unused)]`; shipped so a
  learner who writes the helper the sheet describes does not have to hunt
  for the names.
- `handle`: body stays `todo!("T12")`. Doc comment becomes golden's new one
  (a further line is ignored; a peer closing its side ends the handler at
  once; "see `next_change`").
- `next_change`: golden's private helper. **Do not ship it**, not even as a
  stub. The doc comment on `handle` names it, and the T14 sheet says
  "golden puts the wait in a helper" and that inlining is equally right; the
  learner decides. Shipping a `todo!("T14")` stub for a private fn would
  make the choice for them.
- Everything else in the file: unchanged.

## `tests/`

- `tests/t14_second_pass.rs`: provided whole, verbatim from golden. It uses
  only `tests/common` and public items, and compiles against the scaffold
  (`run_until`'s stub has the signature it needs). All nine tests fail with
  the stubs until T12–T14 are done, like every other provided suite.

## `exercises/`

- `exercises/tasks/T14-second-pass.md`: new, verbatim.
- `exercises/tasks/T12-socket.md`, `T13-tasks.md`, `T15-capstone.md`: the
  `docs(T14)` edits cherry-pick cleanly (a corrected pointer and a forward
  pointer on T12, a forward pointer on T13, the subscriber hang-up bullet
  removed from T15's stretch list and the transcript-replacement bullet
  labelled as T14's stretch).
- `exercises/ANSWERS-golden.md`: golden only, not shipped, as before.

## Nothing else

No `Cargo.toml` change (`sync`, `macros`, `time` and `io-util` were already
on). No change to `effects.rs`, `mod.rs`, `core.rs` or `tests/common`.
`README.md` already lists T14 and needs nothing from this branch.
