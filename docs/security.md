# Security model

Identity, encryption, upload authentication and rate limiting — what each
guarantees, and where the guarantee stops. Authorization (roles, permissions,
private channels, moderation policy) is [`permissions.md`](permissions.md).

Before changing anything in here, read
[`../agents/skills/crypto-changes.md`](../agents/skills/crypto-changes.md).

## Identity

Authentication is an Ed25519 signature over a challenge the server issues
per connection. The server binds an account name to the **first key that
claims it**, permanently.

That single key does more than log you in:

- it authenticates on *every* server, not just one;
- it derives (via ed2curve) the X25519 keys DMs are encrypted to;
- it wraps the per-channel keys of end-to-end encrypted channels;
- it signs the `presence` proof that opens the `/upload` session.

So losing it loses every account at once, plus the ability to read a single
DM ever received. **That is why the key is backed up, not merely stored.**

`murmer_client/src/lib/identity.ts` exports it two ways from the same 32-byte
seed:

- a **passphrase-encrypted recovery file** — PBKDF2-SHA256 over Web Crypto,
  then NaCl secretbox — which also carries the account name;
- a **24-word BIP39 phrase**, whose checksum catches a mistyped word before
  it restores a *valid but different* identity.

An Ed25519 secret key is `seed || publicKey`, which is why `seedOf` can
export identities created before any of this existed: the storage format
never changed. Restoring replaces the key every store and connection was
built around, so it confirms and then reloads.

Nothing in `identity.ts` may reach for `localStorage` or a store. The formats
are a promise to anyone holding an old backup, and `identity.test.ts`
deliberately rebuilds the file from its *documented description* rather than
from the exporter, so a drift in either one fails the suite.

Key pairs live in `localStorage`. That is acceptable for the prototype;
evaluate more secure storage before calling this production-ready.

### Replay protection

Every connection is greeted with `{"type":"auth-challenge","challenge":…}`,
32 random bytes, and `presence` signs `presence:<challenge>`. The challenge
is what binds the proof to one server and one socket.

The key being the account on every server is why that matters. The proof
used to sign only a timestamp, so the operator of any server a user visited
— or anyone reading its traffic — could replay a fresh proof against another
server within the freshness window and log in there as them. A proof over a
value only this server issued, on this socket, is worthless anywhere else.

The ceiling: a malicious server could still *relay* another server's live
challenge to a visiting client and forward the signature. Closing that
needs the client to sign which server it believes it is talking to, and the
server to check that against an address it knows to be its own — which
behind a reverse proxy it does not reliably know.

### Before authentication

Every socket is subscribed to the server-wide broadcast and to the default
channel the moment it connects, before anyone knows who it is. On a
password-protected server the socket loop therefore delivers nothing until
`presence` succeeds — the subscription is not the gate, and a connection that
simply never authenticated used to read every public message live. For the
same reason channel frames are filtered by visibility as they are delivered,
not only at `join`: the default channel can be private, and access to a
channel can be revoked while somebody sits in it.

A keyless presence on a protected server is refused before the password is
compared. Answering a right and a wrong password differently made it an
oracle, and the authentication rate limit only sees key proofs.

A socket that has not authenticated within `AUTH_TIMEOUT` (10 s) is closed.
There is no per-IP cap on connections, so idle sockets were otherwise a
cheap way to run the server out of file descriptors.

## Direct messages

DMs are end-to-end encrypted with NaCl `box` over the X25519 keys derived
from both users' Ed25519 identity keys (`murmer_client/src/lib/dm-crypto.ts`).

The server only validates, stores and relays `nonce`/`ciphertext` pairs
(`validate_sealed_payload`: base64, 24-byte nonce, bounded size) and stores
the frame verbatim. **It must never gain a plaintext DM path.** Clients fetch
a peer's key with the `get-user-key` frame, answered from the `user_keys`
binding; users without a binding — bots, for instance — cannot receive DMs.

That makes the server the key directory, which is trusted **on first contact
only**: `stores/peerKeys.ts` pins each peer's key per server URL, flags a
change, blocks sending until the user explicitly trusts the new key, and
exposes a fingerprint for out-of-band verification. Keep that flow intact
when touching DM code. There is no forward secrecy.

## Encrypted channels

A **private** text channel may additionally be end-to-end encrypted
(`channels.e2ee`, toggled by a manager with `set-channel-e2ee`).

The channel then has one symmetric key per **epoch**, generated on a member's
machine and stored only as per-member `nacl.box` wraps in `channel_keys` —
the server keeps N opaque blobs and can open none of them. Messages carry an
`enc` envelope (`epoch`/`nonce`/`ciphertext`) instead of `text`.

Split of responsibility:

- **Crypto** — `murmer_client/src/lib/channel-crypto.ts`.
- **Policy** — `stores/channelKeys.ts`: open epoch 1, hand the current key to
  a member who lacks it, rotate to a new epoch when a holder is no longer a
  member. This is what the tests there pin down, because *a rotation that
  does not happen looks exactly like a working channel*.
- **Storage** — `ws/handlers/channel_keys.rs` and `db/channel_keys.rs`. The
  server is a dumb store plus a member directory: `get-channel-keys` returns
  the wraps addressed to the requester along with the roster and the current
  epoch's holders; `put-channel-keys` files new wraps.

Three rules in `db::insert_channel_keys` carry the security of the whole
feature, and `tests/channel_keys_test.rs` covers them:

1. a write may only open the **next** epoch or extend an existing one;
2. extending requires the author to already hold a wrap at that epoch;
3. an existing wrap is **never** overwritten — otherwise a member could swap
   another member's wrap for one sealed to a key they control.

The handler additionally refuses any wrap addressed to somebody off the
roster, which comes from `channel_members`, the same `can_view_channel` check
that gates reading. Because the server is the key directory here too, clients
reuse `stores/peerKeys.ts` and **refuse to wrap the channel key for a member
whose identity key changed** under the pin, surfacing those members instead.

Keys live in client memory only — the server holds the durable wraps — and
are dropped on `connect`/`disconnect` so they never cross servers. Messages
keep their sealed `enc` envelope after decryption so a key arriving late can
re-open them; `decryptPending` is the "waiting for the key" state,
`decryptFailed` is the final one.

`channelKeys.ts` is deliberately a **leaf module**: the chat store injects
its transport with `setTransport` rather than being imported back, which is
what keeps the rotation policy unit-testable without a WebSocket.

### Why private-channel only

A channel `@everyone` can read has every account on its roster, so sealing it
would protect nothing.

### What encryption does not cover

All of this is deliberate, and `README.md` points operators here:

- **Server-side search** — there is no text to index.
- **Bots** — `POST /channels/:id/messages` refuses; a bot has no identity key
  to encrypt with.
- **Uploaded file bytes** — only the attachment's name and URL travel sealed.
- **Link previews** and content-derived stats.
- **The profanity filter and the auto-moderation rules** — the server holds
  no text to mask or match. Doing either would mean handing the server the
  plaintext back, so this is a limit of server-side filtering, not a hole to
  plug. Slow mode and the message length cap still apply.
- **Reply quotes** — the server rebuilds them from stored plaintext, so in an
  encrypted channel it sends `replyTo` with an empty snippet and the client
  supplies the quote from inside the ciphertext.
- **Forwarding**, at either end. A forward into a channel is a copy the
  server makes, and it has neither the plaintext of a sealed message nor a
  key to seal a copy with. Letting the client supply the words under a
  server-stamped attribution instead would be the exact forgery the frame
  exists to prevent, so `prepare_forward` refuses. Forwarding into a **DM**
  works and is client-side for the same reason: the attribution travels
  inside the ciphertext as text, which makes it the sender's claim rather
  than the server's — see [`features.md`](features.md).
- **Forward secrecy within an epoch.**
- **Which groups a message pings.** A group mention's `mentions` field
  travels in plaintext beside the envelope, because the server has to
  authorize it — see [`permissions.md`](permissions.md#group-mentions). It
  reveals that a message pinged `@here` or a role, never what it said.
- **Reminders.** A reminder's note is written by its owner and stored in
  plaintext, like a channel topic — a reminder set on a message in an
  encrypted channel therefore only ever holds what its owner typed, never a
  copy of the message. The client deliberately does not pre-fill it from the
  decrypted text, because a rule that applies only in some channels is one
  somebody eventually forgets.

A **scheduled message** is the exception that proves the shape: it is sealed
by the client at compose time and the server stores the envelope opaquely,
exactly as it stores a live one. It goes out under the epoch that was current
when it was written, which is the same epoch a message sent then would have
used — see [`features.md`](features.md).

`handle_chat` and `handle_edit_message` branch on `channel_is_e2ee` and
**reject** a `text`, `image`, `attachment` or `spoiler` field there rather than
stripping it silently: a client that got this wrong has a bug its user must
hear about.

## Upload authentication

`/upload` rides on the WebSocket's authentication rather than repeating it.

Once `presence` succeeds with a key, the connection's challenge is registered
in `AppState::upload_sessions` with the account it proved, and removed when
the socket closes. `upload::authorize` requires that session as the
`session` field. Banned users are rejected. This is how the server password,
the name binding and the challenge's server binding all carry over to
uploads — a signature the client made by itself could carry none of them.

Three ordering details are the whole point of the design:

- the per-IP `check_upload_rate_limit` runs **before the body is touched**;
- the session is checked **before any file bytes are buffered**, which is
  why it is a multipart field *ahead of* the file;
- credentials stay **out of headers**, because a custom header makes the
  request preflighted and production servers run with CORS disabled.

Clients must therefore build the body with `uploadForm` in
`murmer_client/src/lib/upload.ts`. A hand-rolled `FormData` is rejected.

### File validation

Files are validated by size and an extension safe-list; images are
additionally checked by magic bytes, and sound uploads by
`upload.rs::detect_audio_type`. **Active content (HTML, SVG, scripts) is
never accepted.**

The safe-list is grouped into categories (images, documents, archives, audio,
video) in `murmer_server/src/upload.rs` and mirrored in
`murmer_client/src/lib/chat/constants.ts`. Which categories are accepted and
the per-file size cap are server settings (`MANAGE_SERVER`, Server Dashboard
→ Files & Uploads) persisted in `server_settings` and read by `/upload` on
every request.

Settings can only **narrow** the safe-list: unknown category ids are rejected
on write and dropped on read, so no setting can admit active content. The
client copy is cosmetic (picker `accept`, pre-upload check) but is held to
the server's list by `murmer_client/test/server-mirror.test.ts`, which also
asserts neither copy ever admits active content. See
[`../agents/skills/mirrored-constants.md`](../agents/skills/mirrored-constants.md).

Files are streamed to disk after validating type, size and filename.

### Serving files back

`/files` answers from the app's own origin, so the safe-list is not allowed
to be the only thing between an upload and script execution there. Every
response carries `Content-Security-Policy: sandbox` — a file that somehow
rendered as a document would get an opaque origin and no scripts — and
anything that is not an image, audio or video is sent with
`Content-Disposition: attachment`.

`/files` itself is unauthenticated, password or not, and serves files shared
in private and encrypted channels too. What keeps them private is the key:
`upload` puts 128 random bits into every one, so knowing that a file was
posted, and when, is not enough to fetch it. That second header is also what makes an
attachment download at all: the client marks the link `download`, but
browsers ignore that attribute across origins, and the desktop app is always
on a different origin than the server.

## Rate limiting

Authentication and uploads are rate limited per IP, chat per user. Behind a
reverse proxy the socket peer is the proxy, so `security::client_ip` reads
`X-Forwarded-For` — but only from the peers listed in `TRUSTED_PROXIES`.
Anyone can send that header; believing it from a client would hand it a
fresh bucket per request. Left unset behind a proxy, every user shares one
bucket and five failed logins lock the whole server out.

The limits (`MAX_MESSAGES_PER_MINUTE`, `MAX_AUTH_ATTEMPTS_PER_MINUTE`,
`MAX_UPLOADS_PER_MINUTE`, `MAX_FRAMES_PER_SECOND` — documented in
`README.md`)
are read once when the `RateLimiter` is built rather than on every check, so
they take effect at startup.

Underneath those, every connection has a frame budget (`FrameBudget` in
`security.rs`) covering every frame it sends, whatever its type. The message
limit only covers frames that post something; searches, history pages and
reactions still queue on the one database thread, so without the budget a
single client in a loop slowed the server for everyone. It is a token bucket
rather than a window because honest clients are bursty — a reconnect that
rejoins a full voice mesh sends dozens of frames at once — and it is per
connection, not per IP, so a LAN party behind one address does not share it.
A throttled connection gets one `frame-rate-limit` error per run of dropped
frames, not one per frame.

Each limiter map is swept end to end on a timer, and the window for the key
being checked is always pruned on access, so the limit itself stays exact.

All of that time comes from `RateLimiter::clock`, not `Instant::now()`. The
window and the sweep interval are both a minute long, so
`RateLimiter::with_clock(Clock::manual())` plus `Clock::advance` is what lets
`tests/security_limits.rs` reach behaviour a real sleep never could. **A
sweep that silently stopped running would look exactly like a working rate
limiter** — which is why it is worth a test at all.

## Client-side boundaries

- **DOMPurify sanitises Markdown output.** Keep the dependency current.
  `src/lib/markdown.test.ts` guards that boundary and is one of only two
  suites that leave the Node default (`// @vitest-environment jsdom`, because
  DOMPurify needs a DOM). Not happy-dom: it mis-drives DOMPurify's tree walk
  and lets `<script>` through, so the tests would pass on broken output.
- **Avoid `{@html …}`** unless the content is explicitly sanitised.
- **Links in rendered Markdown open outside the app.** A DOMPurify hook in
  `markdown.ts` gives every external link `target="_blank"` and
  `rel="noopener noreferrer"`. Followed in place, a link replaced the app with
  somebody else's page — inside the desktop window, where a copy of the
  backup screen is a convincing way to ask for a recovery phrase.
- **Nothing a member writes makes a reader fetch from a host they chose.**
  Whatever a message embeds, every reader's client loads as it renders, so
  an embed on the poster's own host is a tracking pixel that reports each
  reader's IP address and reading time. Markdown images render as links,
  the DOMPurify config drops every element and attribute that loads a
  resource, a message's `image` is shown only when `serverFileUrl` places it
  under the connected server's `/files/`, and `/link-preview` inlines the
  OpenGraph image as a `data:` URL instead of returning its address. The
  CSP cannot do this job: `img-src` has to allow any server a user adds.
- **A Content-Security-Policy is the second line behind DOMPurify.** The
  desktop shell's is in `tauri.conf.json`; a web client served through
  `WEB_CLIENT_DIR` gets the same policy from `web_client.rs`, held equal by
  `server-mirror.test.ts`. SvelteKit boots each page from an inline script,
  so the server hashes every HTML response as it serves it rather than once
  at startup — a startup hash goes stale when the client is rebuilt under a
  running server, and the symptom is a blank page. A 304 passes untouched:
  its headers replace the cached ones, and a policy computed from its empty
  body would block the cached page's script.
- **Validate server responses before mutating client state.**

## Operational notes

- Admin tokens are compared with constant-time equality.
- Keep CORS disabled in production unless explicitly required.
- Stat recording checks `AppState::stats_enabled` (an in-memory mirror of the
  server-wide toggle) before touching the database, so a server with tracking
  off — the default — runs no query per message. That is a shortcut, not a
  gate: the authoritative double opt-in check stays inside
  `db::record_user_stats`, in the same call that performs the increments.
- Invite links carry their payload — a server-issued invite code, or on a
  hub-built link the server password — in the URL **fragment**, so it never
  reaches a web server's access logs or a `Referer` header. One
  `https://…/invite#…` URL serves both clients: clicking it opens the web
  client's `/invite` route, pasting it into the server hub's address field is
  parsed there (`src/lib/invite.ts`).

## Invite codes

A password-protected server admits a connection on one of three credentials,
tried in that order in `ws/handlers/auth.rs`: the server password, an
invite-granted membership bound to the connecting public key, or a live invite
code. Codes are 12 random bytes, base64url-encoded, and carry an optional
expiry and use limit (`db/invites.rs`).

Three properties are load-bearing, and each exists because the obvious
alternative fails:

- **Membership is checked before the code.** Redemption records the joining
  key in `invite_members`, so a member's reconnect neither spends another use
  nor depends on the invite still existing. Without that, an invite could not
  both be single-use and survive its holder reconnecting.
- **Redemption happens after the name and ban checks, not at the credential
  check.** A presence frame that is going to be rejected must not cost the
  invite one of its uses. The validity check is re-run inside the transaction
  that increments the counter, so two clients racing for the last use cannot
  both be admitted.
- **Membership outlives the invite.** Revoking a code deletes the row, which
  stops future joins; it deliberately does not evict the people who already
  joined through it. Removing one of those is a ban, bound to the same public
  key. The alternative — cascading a revocation into a mass eviction — would
  make revoking a leaked link something an admin hesitates to do, which is the
  opposite of what it is for.

Every rejected redemption answers the same `invalid-invite` error. Telling
"expired" apart from "unknown" would let anyone holding a guess find out
whether it named a real invite; the reason is in the server log instead.
