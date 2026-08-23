# WebSocket frames

Read this before adding or changing a WebSocket message type.

A frame touches both halves of the codebase and four decisions that are easy
to get quietly wrong: who may send it, who may see the result, how it leaves
the server, and what the client does with an untrusted copy of it. The
reference behind this procedure is [`../../docs/protocol.md`](../../docs/protocol.md).

## Before writing anything: answer four questions

1. **Who may send it?** Name the permission. If the answer is "anyone
   authenticated", say so explicitly in the handler — an unstated answer
   reads as an oversight to the next person.
2. **Who may see the result?** If it concerns a channel, it must be filtered
   per recipient; private channels make an unfiltered channel broadcast a
   leak.
3. **Which route out?** Server-wide (`AppState.tx`), channel-scoped (the
   per-channel sender) or direct (`AppState.direct`). Anything addressed to
   one user goes direct — broadcasting it costs every connected client a
   socket write and a parse.
4. **Is it manager-only information?** Then it is an answer to a request, not
   a broadcast. The ban list, the storage report and the profanity word list
   all work this way.

## Server side

1. Add the handler in `murmer_server/src/ws/handlers/<domain>.rs`, or a new
   module if it is a new domain. Register it in the dispatch loop in
   `ws/handlers/mod.rs`.
2. Validate the payload before acting on it. Reuse the helpers in
   `ws/validation.rs` and `ws/helpers.rs` rather than hand-rolling checks.
3. Gate it with `has_permission`, and where a channel is involved
   `can_view_channel`. If the frame **names its channel**, check that channel
   — never the one the connection happens to have joined. Search and the wiki
   reads do it this way for exactly this reason.
4. For a relayed frame, confirm `claims_own_user`. A `target` field narrows
   *who sees* the frame; it is not an authorization check and must never be
   used as one.
5. If it is a moderation or role action, add the hierarchy check
   (`top_position`): strictly outranking the target, and never granting a
   permission the actor lacks.
6. If the frame carries sealed content, shape-check it with
   `validate_sealed_payload` and **reject** — never strip — a plaintext field
   in an encrypted channel.

## Client side

1. Register the handler with `chat.on(type, cb)` in the owning store, and
   make sure `chat.off` is called on teardown.
2. **Validate the frame before mutating any store.** A server frame is
   untrusted input.
3. If the frame reports server-wide settings, treat it as *the server's
   answer, cached*: parse, validate, and reset on disconnect so one server's
   policy never leaks into the next. Never write the store optimistically
   from the UI — send the `set-*` frame and wait for the broadcast to
   confirm. See [`../../docs/client-state.md`](../../docs/client-state.md).
4. If the state is per channel or per peer, namespace it by server URL.

## Tests

- Server: an integration test in `murmer_server/tests/` covering the
  authorization decision, not just the happy path — the denial is the part
  worth pinning. See [`server-tests.md`](server-tests.md).
- Client: a store test for the parsing and correlation, if the frame carries
  anything the store has to match up or namespace. See
  [`client-tests.md`](client-tests.md).

## Finally

- Extend the module doc comment on the handler file.
- If the frame introduces a constant the client also needs, read
  [`mirrored-constants.md`](mirrored-constants.md) — do not simply copy it.
- If it changes anything an operator or bot author can see, update
  `README.md` or `murmer_server/BOT_API.md`.
