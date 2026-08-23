# Feature notes

Subsystems small enough not to need their own document, but each carrying at
least one invariant that is not obvious from the code. Voice, screen sharing,
permissions and encryption have their own files.

## Channel wiki

Server: `ws/handlers/wiki.rs`, `db/wiki.rs`. Client: `src/lib/wiki/`.

Every saved version of a page is kept in `wiki_revisions`, pruned to
`MAX_WIKI_REVISIONS_KEPT` on each save.

- `wiki-history` lists them newest first, **bodies omitted** — 50 revisions
  of a 100 kB page would otherwise be a 5 MB sidebar.
- `wiki-revision` fetches one body.
- `wiki-restore` re-applies an old version through the same compare-and-swap
  as `wiki-update`, which is why it answers with the same `wiki-saved`/
  `wiki-conflict` frames.

A restore is stored as a **new revision** rather than rewinding the counter.
History is only ever appended to, so a restore is itself undoable and a
concurrent editor still loses the CAS instead of being silently overwritten.

Reads name their channel in the frame, so `wiki-get`/`wiki-history`/
`wiki-revision` repeat the `can_view_channel` gate rather than trusting the
joined channel, and `require_wiki_writer` checks it alongside `MANAGE_WIKI`.
`wiki-resolve` stays ungated: it answers only "does this page exist" for
channels addressed by name. See [`permissions.md`](permissions.md).

On the client, `src/lib/wiki/` holds the slug rules that mirror the server's
`validate_wiki_slug`, the `[[wikilink]]` action, and `diff.ts` — the line
diff behind the revision history view. The diff is **pure**: no stores, no
DOM. Its edge cases are invisible in a smoke test and are unit-tested
instead: line numbering per side, the prefix/suffix trim that keeps a typo
fix in a long page cheap, and the matrix guard that degrades to a wholesale
replacement rather than hanging the UI.

## Soundboard

Server: `ws/handlers/soundboard.rs`, `db/soundboard.rs`.

A server-wide shared library gated by two permissions: `MANAGE_SOUNDS`
(upload/rename/delete) and `USE_SOUNDBOARD` (play), the latter part of the
`@everyone` baseline.

**Playback is local on every listener.** The server only authorizes
`play-sound` and fans out a `soundboard-play` frame; each client fetches and
plays the file itself, and nothing is ever mixed into a microphone stream.
That is what makes the per-listener volume, per-sound mute and per-user mute
possible — all local, all persisted per server URL.

`play-sound` requires `USE_SOUNDBOARD`, that the connection is actually in
the named voice channel, `can_view_channel` for it, and that the user is not
server-muted. The frame is filtered per recipient by `channel_scope`/
`channel_frame_hint` like the other voice-scoped frames.

Uploads reuse `/upload` and are **re-validated on registration** by
extension, size (`MAX_SOUND_FILE_BYTES`) and audio magic bytes
(`upload.rs::detect_audio_type`), because any authenticated member may upload
and these clips auto-play on everyone's machine.

Playback carries a **server-side per-user cooldown**
(`SOUNDBOARD_COOLDOWN_MS`); `AppState.soundboard_cooldowns` holds it and is
pruned on disconnect. The client's cooldown is a cosmetic mirror.

`db::migrate_soundboard_permissions` grants the two flags to pre-soundboard
databases once, marker-guarded, so an existing server matches a fresh one.

## Message forwarding

Server: `ws/handlers/messages.rs::handle_forward_message`,
`ws/helpers.rs::prepare_forward` / `forwarded_body`. Client:
`src/lib/chat/forward.ts`.

**A forward into a channel is a copy the server makes.** The client sends a
message id and a destination and nothing else; the words and the
`forwardedFrom` stamp are both read out of the stored row. The author of the
new message stays the forwarder — the stamp is what says whose words these
are. Letting the client name that author would let anyone make any member
appear to have said anything, in front of a room with no way to check; it is
the same reason a reply's quoted snippet is rebuilt rather than trusted.

The copy carries `text`, `image` and `attachment` and **nothing else** from
the source. Reactions, the thread id, the reply, the edit marker and the id
all describe the original posting and would be false on a copy.

Forwarding a forward keeps the *first* attribution rather than nesting one
inside the other. The words are still the first author's, and a chain of
"forwarded from a forward of…" tells the reader nothing.

Three refusals carry the design:

- **The source is view-checked, and the refusal is deliberately ambiguous.**
  Message ids are small integers, so without the check a member could guess
  one, forward it into a channel they can post in, and read a private
  channel's contents out of the result. "You cannot see it" and "it does not
  exist" answer with the same code, so guessing cannot even confirm that a
  hidden message is there.
- **An encrypted channel at either end refuses.** The server holds no
  plaintext of a sealed message and no key to seal a copy with, so the only
  way across that boundary would be to let the client supply the words under
  a server-stamped attribution — the forgery the frame exists to prevent.
- **An ephemeral message refuses.** It was posted on the promise that it
  disappears; a copy without the expiry breaks that promise, and a copy
  carrying it would start a second countdown nobody asked for.

**A forward cannot be edited.** Editing is normally the author's own right,
never a moderator's, precisely because it rewrites somebody's words — but a
forward is the one message whose author did not write it, so that rule stops
protecting anyone there. `may_edit_message` refuses, or forwarding and then
editing would put arbitrary words under somebody else's name with the
server's own attribution still on top of them. Deleting a forward stays
allowed: withdrawing it claims nothing.

**Forwarding into a DM is the client's copy, not the server's.** A DM is
end-to-end encrypted, so the server can read neither the message being
forwarded nor the copy, and there is no field for it to stamp — the
attribution travels inside the ciphertext as text, because a DM's plaintext
*is* its text. A forwarded DM is therefore a claim by its sender. That is the
honest shape for it and not a weakening: in a two-person conversation the
sender could type the same words anyway, so there is nothing a stamp would
protect. See [`security.md`](security.md).

## Profiles, display names and nicknames

Server: `ws/handlers/profile.rs`, `db/users.rs`. Client:
`stores/profiles.ts`.

Profiles live on the `user_keys` binding row: `avatar`, `display_name`,
`nickname`, `about`, and the `created_at` that doubles as "member since".

`set-avatar` and `set-profile` only ever touch the requester's own row;
absent fields are left alone and `null` clears one. Every client gets an
`avatar-snapshot` plus a `profile-snapshot` (all bindings, offline users
included) after auth, and `avatar-update`/`profile-update` broadcasts on
change.

**The display name is cosmetic.** No server code resolves one back to a
user, which is why it needs no uniqueness check. The account name stays the
identity for auth, roles, moderation, DMs and message authorship.

A **per-server nickname** layers on top and wins in `$displayNames`, because
it is the *server's* label rather than the user's. `set-nickname` is the one
field here somebody else may write, and the only reason that is safe is that
a nickname is exactly as cosmetic as a display name:

- setting your own needs nothing beyond being authenticated;
- setting anybody else's needs `MANAGE_NICKNAMES` **and** strictly
  outranking them — the same hierarchy check the moderation handlers run.
  Without it a fresh moderator could relabel the owner.

Letting the display name beat the nickname would hand the target a one-click
undo, which is why the resolution order is what it is.

It writes through `db::set_user_nickname`, its own statement rather than a
fourth parameter on `set_user_profile`, so an authorized nickname change can
never carry a display name or "about" edit along with it. It travels in its
own frame for the same reason: the two are authorized differently.

`db::migrate_nickname_permissions` grants the flag to roles that already hold
`KICK_MEMBERS` once, marker-guarded, so an existing server's moderators match
a freshly seeded one. `@everyone` never gains it.

Rendering rules for the client are in [`client-state.md`](client-state.md).

## Stats

Lifetime user stats are double opt-in and aggregate-only — see the
privacy-gated features section of [`permissions.md`](permissions.md).

## Bots

A REST API, documented for its users in
[`../murmer_server/BOT_API.md`](../murmer_server/BOT_API.md) and implemented
in `murmer_server/src/bot/`.

A bot has no identity key, which has one hard consequence: it cannot post
into an end-to-end encrypted channel, and `POST /channels/:id/messages`
refuses rather than silently downgrading. See [`security.md`](security.md).
