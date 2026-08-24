# Client state

How `murmer_client/src/lib/stores/` is organised, and the three distinctions
that decide where a new piece of state belongs.

## Stores are module-level singletons

Every store wires itself up on import. That has one consequence worth knowing
before you write a test: a suite that needs a fresh instance takes it with
`vi.resetModules()` plus a dynamic `import()`, and mocks `./chat` rather than
pulling in the WebSocket manager. See
[`../agents/skills/client-tests.md`](../agents/skills/client-tests.md).

Cross-component state lives in `svelte/store` modules and is consumed with
`$store` auto-subscription — not in component-level runes. Runes
(`$state`/`$props`/`$derived`/`$effect`) are for state that belongs to one
component.

## The connection

`stores/chat.ts` owns the `WebSocketManager`. Register frame handlers with
`chat.on(type, cb)` and clean them up with `chat.off`.

Leaf modules that need to *send* — `channelKeys.ts` is the example — get
their transport injected (`setTransport`) rather than importing `chat` back.
That is what keeps their policy unit-testable without a WebSocket.

## Three kinds of state

### 1. Local preference

Persisted to `localStorage` under `murmer_*` keys. Settings, per-sound mute,
per-user mute, screen-share window layouts.

**Per-channel and per-name state must be namespaced by server URL.** Channel
ids are only unique per server, so last-read markers, notification
preferences and window layouts all carry the server URL in their key. Follow
that pattern for any new per-channel or per-peer persistence — this is the
most common way a new store goes subtly wrong.

### 2. The server's answer, cached

Server-wide settings are **never** local state.
`stores/{chatSettings,uploadConfig,voiceDefaults,screenShare,serverIdentity}.ts`
each:

- parse the frame the server sends after authentication and broadcasts on
  change,
- validate it before mutating anything,
- and **reset on disconnect**, so one server's policy never leaks into the
  next.

The Server Dashboard edits them by sending a `set-*` frame and waiting for
the broadcast to confirm. It never writes the store itself.

Some of these are answers to a request rather than broadcasts, because they
are manager-only: the profanity word list (`chat-settings` in reply to
`get-chat-settings`), `stores/automod.ts`, `stores/bans.ts`,
`stores/storageUsage.ts`, `stores/auditLog.ts` and
`stores/serverMetrics.ts`. Their "not disclosed yet" state is `null`, which
is deliberately **not** the same as "empty" — an editor must not offer to
save an empty list over a real one, and a health readout must not show a
server it was refused as one doing nothing.

### 3. Trust state

`stores/peerKeys.ts` pins each peer's identity key per server URL and is
consulted by both DM encryption and channel-key wrapping. It is the one store
whose contents are a security decision. See [`security.md`](security.md).

## Names

Render `$displayNames(user)` from `stores/profiles.ts`. Never render the raw
user name — that is the **account name**, and it stays the key for every
lookup (`$roles[user]`, `$avatars[user]`, DM peers, mentions).

`$displayNames` resolves **nickname → display name → account name**. The
nickname comes first because a moderator with `MANAGE_NICKNAMES` may have set
it; a component that reached for `profile.displayName` itself would quietly
render the name the server was overriding.

`UserProfileModal` shows both and is where a user edits their own display
name, nickname, about text and avatar. The user context menu is where a
moderator changes somebody else's nickname.

## Composer drafts

`stores/drafts.ts` parks unsent composer text per conversation —
`channel:<id>`, `thread:<rootId>`, `dm:<account>` — so switching away
mid-sentence and back keeps the sentence. One composer serves every channel
and one panel every thread and DM, so the text has to be moved deliberately:
the leaving side calls `park`, the arriving side `take`. `take` removes the
entry, which is what keeps the store describing only the conversations the
user is *not* looking at, and means a send has nothing to clean up.

It is namespaced per server URL like the other per-channel state, but is
**session-local and never persisted**, which is the one thing to preserve if
this is ever extended. Channel keys are held in memory precisely so an
encrypted channel leaves nothing on disk; writing its draft to
`localStorage` would put that message's plaintext exactly where its
ciphertext never goes. Namespacing per server is still what makes a
reconnect restore drafts instead of dropping them.

## Encrypted-channel keys

`stores/channelKeys.ts` holds channel keys **in memory only** — the server
holds the durable wraps — and drops them on `connect`/`disconnect` so they
never cross servers. Messages keep their sealed `enc` envelope after
decryption so a key arriving late can re-open them: `decryptPending` is the
"waiting for the key" state, `decryptFailed` the final one. Policy details
are in [`security.md`](security.md).
