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
server-muted. The frame is filtered per recipient by its `Route` like the
other voice-scoped frames.

Uploads reuse `/upload` and are **re-validated on registration** by
extension, size (`MAX_SOUND_FILE_BYTES`) and audio magic bytes
(`upload.rs::detect_audio_type`), because any authenticated member may upload
and these clips auto-play on everyone's machine.

Playback carries a **server-side per-user cooldown**
(`SOUNDBOARD_COOLDOWN_MS`); `AppState.soundboard_cooldowns` holds it and is
pruned on disconnect. The client's cooldown is a cosmetic mirror.

`db::migrate_soundboard_permissions` grants the two flags to pre-soundboard
databases once, marker-guarded, so an existing server matches a fresh one.

## Raised hands

Server: `ws/handlers/hands.rs`. Client: `stores/voiceHands.ts`.

A member of a voice channel may raise or lower their own hand, and nobody
else's. The server stamps when a hand went up, so the speaking queue is
ordered by its clock rather than by whatever a client reports, and raising an
already raised hand keeps its place.

Hands live in memory and **drop when their owner leaves or switches
channel**. The server broadcasts that as a lowered hand; without it everyone
else would keep showing the hand of somebody who left. `voice-hand` is
channel-scoped like the other voice frames, and the snapshot
(`voice-hands-active`) goes to whoever joins and replaces what they still
held for that channel from an earlier visit.

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

## Message links

Client: `src/lib/message-link.ts`, with the jump itself in
`routes/chat/+page.svelte` (`openMessageLink`).

A link is an ordinary `https://<host>/chat#server=…&channel=…&message=…`,
shaped like an invite link so one link works in a browser and inside the
app. Clicked in a message, the chat page catches it before the desktop
shell's opener would hand it to the system browser.

**A link only opens a server the user already added.** Connecting hands a
server the account name, the public key and the user's address, so a link
anyone can post must not be able to start that. A link for an unknown
server is refused with a message instead of offering to add it — adding a
server is what invite links are for.

The link names a channel, not a permission: one into a channel the reader
cannot see stops at "a channel you cannot see", and the history request
behind the jump is checked by the server like any other.

## Spoilers

Client: `src/lib/spoilers.ts`, the `||…||` extension in `markdown.ts`.
Server: `spoiler` in `PLAINTEXT_MESSAGE_FIELDS`.

Revealing is one document-wide listener rather than a handler per surface,
so the channel, threads, DMs and wiki pages all honour `||…||` the moment
they render markdown. The first click on a hidden spoiler only reveals it:
a link inside one must not be followed sight unseen.

**Plain-text previews blank spoilers instead of rendering them.** OS
notifications, reply quotes and the pinned bar show text the app cannot
blur, so `hideSpoilers` replaces each one with `[spoiler]`. `notify()`
applies it itself, which keeps a new notification from forgetting to.

**An image spoiler is a flag on the message, sealed like the image.** In an
encrypted channel it travels inside the envelope — it says something about
the content — so the server rejects it in the clear there, as it does the
image itself. It is one of the fields a forward copies; a forward that
dropped it would show the image unblurred in the next channel.

## Search filters

Client: `parseSearchQuery` in `src/lib/chat/search.ts`. Server:
`search_filters` in `ws/handlers/messages.rs`, `SearchFilters` in
`db/messages.rs`.

The search box takes `from:`, `in:`, `has:file`, `before:` and `after:`
next to its words. `in:` never reaches the server: it only picks which
channel the frame names, and the server checks that channel as it always
did. The rest travel as a structured `filters` field rather than inside the
query text, so the FTS sanitiser never has to know about them.

**`from:` is the account name.** It matches the `user` the server stamped on
the message, and a display name could name several people.

**Dates are compared as text.** The client turns a day into a UTC instant
at the user's local midnight; the server re-stamps it with the same
`to_rfc3339` that writes `timestamp` on every message, and that shared shape
is what makes a string comparison in SQLite correct.

**A filter alone is a search,** and it reaches into encrypted channels:
author and timestamp are plaintext metadata there anyway. `has:file` does
not, because the attachment is sealed. A filter the client cannot honour (an
unknown channel, an impossible date) is an error, not dropped — searching
without it would answer a different question.

## Text-to-speech

Client: `src/lib/tts.ts`, `/tts` in `chat/commands.ts`, the toggle in
Settings → Audio. No server code: `tts: true` is stored like any other
field the client sends.

**The sender asks, the listener decides.** The toggle is off by default and
lives in `localStorage`, and a channel muted in its notification settings
stays silent, so one member's `/tts` cannot make a room talk that did not
opt in.

**Only live messages speak.** The chat store hands each opened live message
to `tts.ts` through `onLiveMessage`; history, threads and search never pass
there, so scrolling back does not replay a conversation.

**The flag is plaintext in an encrypted channel.** It travels beside the
envelope, like the reply id, because it says how to deliver the words and
not what they are. The words themselves are read from the opened message,
with spoilers replaced by `[spoiler]` as in a notification.

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

**Mentions name the account, too.** `containsMention` (`message-utils.ts`)
matches `@accountname` only — a display name or nickname is not unique, so
it cannot say who was meant. People type the name they *see*, though, so the
composer and thread replies complete `@`: members are found by their shown
name (`chat/mentions.ts`) and the account name is what gets inserted. Typing
`@Nick` by hand without picking from the list still notifies nobody.
Group mentions — `@here` and roles — are permission-gated and work
differently; see [`permissions.md`](permissions.md#group-mentions).

The **mentions inbox** lists recent mentions across every channel, for the
reader who comes back to a red badge and wants the *what*, not just the
*where*. It is the server's answer to `load-mentions`
(`ws/handlers/mentions.rs`), built from channels the reader can see right
now, so losing access to a private channel takes its mentions along. Two
choices in it are deliberate:

- **The rule is stated twice.** Clients decide live pings with
  `containsMention`; the server decides the inbox with
  `mentions::mentions_user`. A mention one of them sees and the other does
  not looks like a lost message, so the Rust tests run the client's own
  cases.
- **`@here` is not kept.** It reached whoever was connected when it was
  sent; listing it later to someone who was not would make every `@here` an
  `@everyone`. Role pings are kept — holding the role is the whole point.

In an encrypted channel the server has no text, so it can only find role
pings there. Name mentions in one reach the inbox live, from the client's own
decrypted copy, while the reader is connected (`stores/mentionInbox.ts`).
The inbox is held in memory and reset per connection, like the drafts: it
carries the plaintext of encrypted channels, which must never land on disk.

Rendering rules for the client are in [`client-state.md`](client-state.md).

## Reminders and scheduled messages

Two features over one background timer
(`ws/handlers/scheduled.rs`, `db/scheduled.rs`). A **reminder** is a private
note the server hands back to its owner at a chosen time; a **scheduled
message** is an ordinary channel message written now and posted later, as its
author. Both are reachable from `/remind`, `/schedule` and the Reminders panel
in the header, and a reminder can also be set on a specific message.

### Why one scheduler, and why it polls

One task drains both queues every `SCHEDULER_TICK_SECONDS`, rather than a
`tokio::spawn` sleeping per row the way ephemeral deletion does. A per-row
timer lives only in memory, which is why ephemeral messages need
`resume_ephemeral_deletions` to re-arm after a restart; a poll over an indexed
column needs no such sweep, because the database *is* the queue. The minimum
lead time is at least one tick, so the polling interval is never visible as
lateness.

### Screened when written, authorized when posted

A scheduled message clears `authorize_send` and `prepare_chat_body` — the same
two steps `handle_chat` runs — at the moment it is composed. Screening belongs
there because that is the only moment its author is present to hear a refusal;
a message the auto-moderator would block should not sit in a queue for a week
first.

Authorization is then re-checked at delivery, and this is the part that is
easy to leave out: between writing and posting, the author can lose
`SEND_MESSAGES`, lose sight of the channel, be muted, or watch the channel
switch to end-to-end encryption. The last one matters most — a plaintext
message queued before the switch would make the *server* the thing putting
plaintext into an encrypted channel. A message that fails any of these is kept
and marked failed, so its author finds out; it is never posted, and never
silently dropped.

### Exactly-once, and what happens when it is not

Delivery costs are asymmetric: a reminder that fires twice is a nuisance, a
message that posts twice says something once and shows it twice. So:

- A **reminder** is claimed by stamping `fired_at`, and the row is *kept*
  until its owner dismisses it. An owner who was offline when it fired finds
  it waiting, already marked due, at their next `presence`.
- A **scheduled message** is claimed by stamping `claimed_at`, which takes it
  out of reach of every later tick before anything is published. Only a
  delivery that happened deletes the row.

A process that dies between the claim and the post leaves a claimed row that
may or may not have been sent. `fail_claimed_scheduled_messages` runs at
startup and turns those into a visible failure rather than a second post —
telling the author is recoverable, posting twice is not.

### Encrypted channels

A scheduled message for an encrypted channel is sealed on the client at compose
time, under the epoch that is current then, and the server stores the envelope
exactly as it stores a live one. That is the same epoch a message sent at that
moment would have used, so who can read it does not depend on when the queue
drains. Reminders are *not* encrypted — see
[`security.md`](security.md).

### Bounds

Per-user caps (`MAX_SCHEDULED_MESSAGES_PER_USER`, `MAX_REMINDERS_PER_USER`)
count failed and fired rows too, because those still occupy storage until they
are cleared. The Danger Zone reset drops queued messages — they are addressed
to channels, and a reset is exactly the moment those channels stop meaning what
they meant — but keeps reminders, which are personal notes rather than server
structure.

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
