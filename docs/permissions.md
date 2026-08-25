# Permissions and moderation

Who may do what: the permission bitmask, the role hierarchy, per-channel
overrides, the server-enforced chat policy and the Danger Zone. Encryption
and identity are [`security.md`](security.md).

The rule that governs this whole document: **the server is the only
enforcement point.** `has_permission` and `top_position` in
`murmer_server/src/ws/helpers.rs` decide; client gating is cosmetic.

## The bitmask

Authorization is a **permission bitmask**, not fixed roles. The flag set is
defined in `murmer_server/src/permissions.rs` — the authority — and mirrored
in `murmer_client/src/lib/chat/permissions.ts`. Keep them in sync;
`murmer_client/test/server-mirror.test.ts` parses the Rust source and fails
when the two drift. See
[`../agents/skills/mirrored-constants.md`](../agents/skills/mirrored-constants.md).

Server owners define custom roles in the Server Dashboard and toggle each
capability (view/send/manage channels/kick/ban/manage roles/…) per role. A
user's effective permissions are the **union** of the built-in `@everyone`
baseline and every role assigned to them. `ADMINISTRATOR` (the Owner role)
grants everything.

Without `ADMIN_TOKEN`, channel and wiki management stay open to everyone —
the historical fallback, kept so a small unadministered server remains
usable. Every other capability is role-gated regardless.

## Roles and hierarchy

Roles are `role_definitions` rows carrying a permission mask and a hierarchy
`position`; users hold any number of them (`user_roles`). Two rules bound
what a manager can do, and both exist to prevent escalation:

- moderation and role management require **strictly outranking** the target;
- a manager can never grant a permission it does not itself hold.

Role CRUD and assignment flow through the `create-role`/`update-role`/
`delete-role`/`reorder-roles`/`set-user-roles` frames
(`ws/handlers/roles.rs`), all requiring `MANAGE_ROLES`.

`/role` and the `set-role` CLI add a named role to a key, creating the
definition if missing — that is the bootstrap path for the first Owner, and
the only thing `ADMIN_TOKEN` still gates. Legacy single-role databases are
migrated once by `db::migrate_roles`.

### Role icons

`update-role` carries an optional role `icon`: an `/files/<key>` upload URL
pointing at an uploaded image or an existing custom emoji's file. It is
validated on write like the server icon (image safe-list, file must exist,
`MAX_ROLE_ICON_BYTES`) and sanitized again client-side before being used as
an image source.

Replaced icon files are **intentionally left on disk**, because the same file
may back a custom emoji or another role.

## Private channels

Private channels layer per-channel allow/deny overrides — for `@everyone`,
for roles and for individual users — on top of the server-wide permissions,
resolved by `channel_permissions`/`can_view_channel` in `ws/helpers.rs`.

Overrides only touch two flags, clamped to `CHANNEL_OVERRIDABLE`: "see"
(`VIEW_CHANNELS`) and "write/talk" (`SEND_MESSAGES`).

Enforcement is spread across every path that could leak a channel's
existence or content:

- channel-list senders are viewer-aware;
- channel-scoped broadcasts are filtered per recipient in the `global_rx`
  loop (`ws/handlers/mod.rs`), from a per-connection memo whose invalidation
  is [`protocol.md`](protocol.md);
- join, history, search, send, react, pin, `voice-join` and every wiki read
  and write are channel-gated.

Search and the wiki reads name their channel **in the frame**, so they repeat
the history gate rather than trusting the joined channel — and the search
gate covers the wiki page hits it answers with as well. A private channel's
pages are as much its content as its messages are, which is why
`require_wiki_writer` checks visibility alongside `MANAGE_WIKI`.
`wiki-resolve` stays ungated: it answers only "does this page exist" for
channels addressed by name.

**Voice talk is the one client-enforced piece** (the mic is disabled via the
`voice-permissions` hint) because audio is peer-to-peer. View and join, and
every text gate, are server-enforced.

Managers (`MANAGE_CHANNELS`) edit overrides through the
`set-channel-override`/`remove-channel-override`/`get-channel-overrides`
frames (`ws/handlers/channel_overrides.rs`); override data is sent only to
managers. Creating a channel with `private: true` seeds an `@everyone`
View-deny plus a creator allow.

Encrypting a private channel is [`security.md`](security.md).

## Chat policy

Server Dashboard → Moderation (`MANAGE_SERVER`) adds three server-enforced
limits on top of the rate limiter:

| Setting | Behaviour |
| --- | --- |
| **Slow mode** | Per-user send interval. Members with `MANAGE_MESSAGES` are exempt. Timestamps live in `AppState.slow_mode_sends` and are dropped on disconnect — it paces a live conversation rather than punishing someone across sessions. |
| **Message length cap** | May only *narrow* the built-in `MAX_MESSAGE_LENGTH`, never widen it. |
| **Profanity filter** | Masks matched words per whole word, case-insensitively (`profanity.rs`), so a filter for "ass" leaves "class" alone. |

The filter masks in `handle_chat` **and** `handle_edit_message`, before the
message is stored or broadcast, so posting and immediately editing cannot
walk past it and the original never reaches another client.

All three are cached in `AppState.chat_settings` because every message
consults them; `ws/handlers/chat_settings.rs` refreshes that cache in the
same step that writes the row. They are clamped both on write and on read
(`db::clamp_chat_settings`), so a hand-edited row cannot widen them.

The client copies — composer `maxlength`, the slow-mode hint — are cosmetic
and mirrored by `murmer_client/test/server-mirror.test.ts`.

In an end-to-end encrypted channel the profanity filter is a no-op; slow mode
and the length cap still apply. See [`security.md`](security.md).

## Manager-only answers

Some data is an answer to a request, never a broadcast, because it is
manager information:

- the **ban list** (`BAN_MEMBERS`) — its rows carry public keys;
- the **storage usage** report (`MANAGE_SERVER`);
- the **profanity word list** — the public `chat-settings` broadcast
  deliberately omits it, and it is answered only to `get-chat-settings`.

On the client these live in `stores/bans.ts`, `stores/storageUsage.ts` and
`stores/chatSettings.ts`, where "not disclosed yet" is `null` — deliberately
not the same as "empty", so an editor cannot offer to save an empty list over
a real one.

## Danger Zone

Purging all messages and resetting the server (`db/maintenance.rs`,
`ws/handlers/maintenance.rs`) require `ADMINISTRATOR` **and** the
confirmation phrase (`PURGE`/`RESET`) echoed in the frame, and run their
deletions in one transaction.

A reset deliberately keeps `@everyone` and the Owner role — deleting them
would leave the server with nobody able to administer it — along with
identities, bans, mutes, emojis, sounds and stats.

It must also clear the **in-memory mirrors** of what it deleted
(`voice_channels`, `channel_overrides`, `role_defs`, `user_roles`) and
broadcast `channels-refresh`, or a deleted channel stays joinable and a
deleted role keeps granting permissions until the next restart.

A reset additionally clears every channel key and the surviving `general`
channel's `e2ee` flag: its overrides are gone by then, so leaving the flag
set would strand a channel marked encrypted that is no longer private.

## Privacy-gated features

Lifetime user stats are **double opt-in**: recording requires the
server-wide toggle (Owner/Admin) *and* the user's own opt-in, enforced in
`murmer_server/src/db/stats.rs`. Only aggregate counters are stored — never
message contents or recipients.

The soundboard's `play_count`/`last_played_at` live on the sound row instead
and are deliberately unattributed, so they need no consent gate. **Never add
a "who played it" column.**
