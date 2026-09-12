# T11 — async II: the socket

**Core: 1h** · Gaps: `AsyncRead` / `AsyncWrite` generics, `into_split`,
`AsyncBufReadExt::lines`, `spawn_blocking` and `'static` closures, `Arc<Path>`,
`try_send` versus `send` · Test: `cargo test --test t11_rpc`

A hook connection sends one line and waits for one verdict. A subscriber sends
one line and then reads forever. Nothing here may stall an agent, which is why
the inbox is offered to with `try_send` rather than waited on.

## Implement (`src/daemon/rpc.rs`)

- `handle<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(read, write, inbox, view, proc_root)`
  - Cap the connection with `read.take(MAX_CONNECTION_BYTES)`, then loop over
    `lines()`.
  - Each line goes through `decode_request`. The reply depends on what came
    back:
    - Errors get `error_reply`.
    - `Hook` and `Statusline` get resolved to a process, run through the
      adapter, `offer`ed to the inbox, then `ok_reply`.
    - `List` gets a snapshot line.
    - `Subscribe` writes a snapshot now, then one more on every
      `view.changed()`, until the peer or the view goes away.
- `offer`: `try_send`, and log on a full inbox.
- `resolve(proc_root: &Arc<Path>, pid, view) -> Option<ProcKey>`
  - Copy the tracked keys out of `view.borrow()` **before** anything that
    could await. A `watch::Ref` must not be held across an `.await`.
  - Walk `ancestors` inside `spawn_blocking`. The closure has to be
    `'static`, so it needs an owned `Arc` clone and an owned `Vec`.
- `serve` and `write_line` are small; do them too.

`daemon::bind` is provided. Read how it refuses to steal a live daemon's
socket and clears a dead one's.

## Katas

1. `handle` is generic over its two halves instead of taking a `UnixStream`.
   What does that buy in `t11_rpc`, and what does `Unpin` have to do with
   calling `.read()` / `.write_all()` on a `&mut W`?
2. What happens if you hold `let r = view.borrow();` across
   `spawn_blocking(..).await`? Is it a compile error or a runtime problem, and
   why?
3. The forwarder waits 500 ms at most. With `inbox.send(input).await` instead
   of `try_send`, describe the chain of events when the core is slow and ten
   agents fire hooks at once.

## Done when

`t11_rpc` passes. That includes a real Unix socket served end to end to the
synchronous `client::send` from T02.
