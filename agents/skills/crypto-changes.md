# Crypto changes

Read this before touching identity keys, DM encryption, channel encryption,
key pinning or the identity backup formats.

The model is [`../../docs/security.md`](../../docs/security.md). This file is
about what will go wrong.

## What is at stake

One Ed25519 key is the account **on every server at once**, and it also
derives the X25519 keys DMs are encrypted to and wraps every channel key.
Breaking it does not produce an error message — it produces users who cannot
log in anywhere and cannot read anything they ever received.

Three failure modes are silent by nature and are what the rules below defend
against:

- **A rotation that does not happen** looks exactly like a working channel.
- **A wrap sealed to the wrong key** looks exactly like a member who has not
  fetched their key yet.
- **A backup format that drifted** only fails years later, on the one day
  somebody actually needs it.

## The rules

### The backup formats are a promise

`murmer_client/src/lib/identity.ts` exports the 32-byte seed two ways: a
passphrase-encrypted recovery file (PBKDF2-SHA256 over Web Crypto, then NaCl
secretbox, carrying the account name) and a 24-word BIP39 phrase whose
checksum catches a mistyped word before it restores a *valid but different*
identity.

Someone out there has a file written by an old version. **Changing either
format silently is a data-loss bug.** If a change is genuinely needed, it has
to read the old format too — which is the one place in this repo where
backwards compatibility is not optional.

`identity.test.ts` rebuilds the file from its *documented description* rather
than from the exporter, so a drift in either the code or the documentation
fails the suite. Keep it that way: a test that calls the exporter would pass
on any format.

Nothing in `identity.ts` may reach for `localStorage` or a store.

### Never bypass the pin

`stores/peerKeys.ts` pins each peer's identity key per server URL. It is
consulted by both DM sending and channel-key wrapping, and both **must**
refuse when the pin does not match — DMs block until the user explicitly
trusts the new key, channel-key wrapping surfaces the member instead of
wrapping for them.

The server is the key directory. Pinning is the only thing standing between
that and a server that hands out its own key for a user.

### The three channel-key rules

In `db::insert_channel_keys`, covered by `tests/channel_keys_test.rs`:

1. a write may only open the **next** epoch or extend an existing one;
2. extending requires the author to already hold a wrap at that epoch;
3. an existing wrap is **never** overwritten — otherwise a member could swap
   another member's wrap for one sealed to a key they control.

Plus: the handler refuses any wrap addressed to somebody off the roster.

If you are changing this file, extend that test in the same commit.

### No plaintext path

For DMs and for `e2ee` channels the server only ever sees base64
`nonce`/`ciphertext` pairs, shape-checked by `validate_sealed_payload`.

Never add a code path that accepts or produces plaintext there. When a client
sends a `text`, `image` or `attachment` field into an encrypted channel,
`handle_chat` and `handle_edit_message` **reject** the frame rather than
stripping the field: a client that got this wrong has a bug, and its user
needs to hear about it rather than believe a message was sealed when it was
not.

### Policy stays in a leaf module

`stores/channelKeys.ts` owns *when* to rotate and *who* to wrap for. It takes
its transport by injection (`setTransport`) rather than importing `chat`
back, which is the only reason its policy is unit-testable without a
WebSocket. Do not make it depend on the chat store.

## Before finishing

- Extend the tests that pin the behaviour you changed —
  `identity.test.ts`, `peerKeys.test.ts`, `channelKeys.test.ts`,
  `tests/channel_keys_test.rs`.
- Document the reasoning inline. The repo asks for inline comments on
  security-sensitive logic specifically because these paths look arbitrary
  to a reader who does not know the attack.
- Re-check the "what encryption does not cover" list in
  [`../../docs/security.md`](../../docs/security.md) and the matching section
  in `README.md`. If your change moves that boundary, both are now wrong.
- Manually exercise the flow. Key changes are the kind of thing whose failure
  mode is a user who cannot get back in.
