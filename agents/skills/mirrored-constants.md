# Mirrored constants

Read this before changing anything defined in both the Rust server and the
TypeScript client.

Five tables exist twice. The server's copy is the authority and the client's
copy is cosmetic — a picker's `accept` attribute, a composer's `maxlength`, a
greyed-out button. But a drift between them does not fail loudly where it
happens: the symptom shows up far away, as a permission bit that means one
thing to the client and another to the server, or a file picker offering an
extension `/upload` rejects.

## The five pairs

| Authority (Rust) | Mirror (TypeScript) |
| --- | --- |
| `murmer_server/src/permissions.rs` | `murmer_client/src/lib/chat/permissions.ts` |
| `murmer_server/src/upload.rs` (categories, size bounds) | `murmer_client/src/lib/chat/constants.ts` |
| `murmer_server/src/db/chat_settings.rs` (bounds) | `murmer_client/src/lib/chat/constants.ts` |
| `murmer_server/src/db/voice_defaults.rs` | `murmer_client/src/lib/chat/constants.ts` |
| `murmer_server/src/automod.rs` (bounds, kind/action names) plus the mute bounds in `db/moderation.rs` | `murmer_client/src/lib/chat/constants.ts` |

## The guard

`murmer_client/test/server-mirror.test.ts` reads those Rust files **as text**
and asserts the client's copies still match. It also asserts that neither
copy ever admits active content (HTML, SVG, scripts) to the upload
safe-list.

Reading the Rust as text keeps this a plain Vitest run with no toolchain of
its own. The trade-off is deliberate: the test is coupled to the *formatting*
of those files, so it carries parse assertions that fail loudly if a rewrite
makes a regex stop matching — rather than silently comparing nothing.

## Procedure

1. Change the **Rust** side first. It is the authority; the client's copy is
   never the reason the server behaves a certain way.
2. Change the TypeScript mirror to match.
3. Run `bun run test` in `murmer_client/`. If `server-mirror.test.ts` fails
   with a *parse* assertion rather than a comparison, your Rust edit changed
   the shape the regex expects — fix the regex in the same commit, and make
   sure it still fails when the values genuinely differ.
4. If you added a new mirrored value, extend `server-mirror.test.ts` to cover
   it. An unguarded mirror is the situation this test exists to prevent.

## Rules that constrain the values themselves

- **Upload categories may only narrow.** Unknown category ids are rejected on
  write and dropped on read, so no server setting can admit active content.
- **The message length cap may only narrow** the built-in
  `MAX_MESSAGE_LENGTH`. `db::clamp_chat_settings` enforces that on write
  *and* again on read, so a hand-edited database row cannot widen it.
- **Permission bits are positional.** Reusing or reordering a bit changes
  what every stored role mask means. Add new flags at the end.
- **An auto-moderation kind or action is its wire name.** The server rejects
  one it does not know rather than falling back to a default, so a client
  offering a name spelled differently saves nothing and says nothing. The
  action list is also ordered least to most severe on both sides, because
  that ordering is what decides between two rules matching one message.

## What is not mirrored, and should not become mirrored

Environment variables are documented in `README.md` and read in
`murmer_server/src/config.rs`. Do not add a client-side copy of a server
default; ask the server for it as a settings frame instead — that is what
`stores/uploadConfig.ts` and friends already do.
