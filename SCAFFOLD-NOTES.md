# Scaffold notes for the T10–T13 fixes

Everything in `src/` on this branch changed outside function bodies (doc and
module comments only; no imports, no signatures), so each edit applies to
the scaffold verbatim. One fix exists only in the scaffold.

## Scaffold-only

### `src/daemon/effects.rs`, `impl Drop for Watchers`

The scaffold currently has, inside `drop`:

```rust
        // Scaffold guard: a panic inside `drop` while a failing test is
        // already unwinding aborts the whole test binary, so the `todo!()` in
        // `abort_all` is only reached when it can fail cleanly. Keep or remove
        // the guard once `abort_all` is implemented.
        if std::thread::panicking() {
            return;
        }
        self.abort_all();
```

It should have:

```rust
        // Scaffold guard: a panic inside `drop` while a failing test is
        // already unwinding aborts the whole test binary, so the `todo!()` in
        // `abort_all` is only reached when it can fail cleanly. Remove the
        // guard once `abort_all` is real: `JoinHandle::abort` never panics.
        if std::thread::panicking() {
            return;
        }
        self.abort_all();
```

The T12 sheet now says the guard can go once `abort_all` is implemented,
and the comment should say the same rather than "keep or remove".

## Mirror to the scaffold (same edit, outside bodies)

| File | Where | Commit |
|---|---|---|
| `src/daemon/core.rs` | module comment: cancellation-safety paragraph | `docs(T10)` |
| `src/daemon/rpc.rs` | doc comment on `handle` (subscribe semantics, the `Ref` rule); doc comment on `resolve` (nearest tracked ancestor of `pid`, `pid` included; matched by pid) | `docs(T11)` |
| `src/daemon/watcher.rs` | doc comment on `run` | `docs(T12)` |
| `src/daemon/effects.rs` | new doc comment on `run` | `docs(T12)` |
| `src/daemon/scanner.rs` | doc comment on `sweep_inputs` | `docs(T12)` |
| `src/daemon/publish.rs` | doc comment on `write_atomic` | `docs(T12)` |
| `tests/common/mod.rs` | doc comment on `key` | `docs(T11)` |
| `tests/t11_rpc.rs` | comment in `should_ignore_bytes_beyond_the_connection_cap` | `docs(T11)` |
| `tests/t12_tasks.rs` | new test `should_stop_when_the_inbox_closes_while_the_transcript_is_quiet`; doc comment on `sentinel` | `test(T12)` |

The sheets (`exercises/tasks/T10` to `T13`) cherry-pick cleanly.
