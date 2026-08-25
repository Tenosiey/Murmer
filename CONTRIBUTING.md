# Contributing to Murmer

Thanks for considering a contribution. Murmer is maintained by one or two
people, which shapes most of the guidance below: the simpler change is the
one that gets merged.

## Getting set up

You need [Rust](https://www.rust-lang.org/tools/install) (the toolchain is
pinned by `rust-toolchain.toml`, so rustup picks the right one) and
[Bun](https://bun.sh) 1.x. `README.md` has the full setup, including Docker.

```bash
cd murmer_server && cargo run
```

```bash
cd murmer_client && bun install && bun run tauri dev
```

For the fastest loop — and the only setup that exercises the web client — let
the server serve the built client instead:

```bash
cd murmer_client && bun run build
```

```bash
cd murmer_server && WEB_CLIENT_DIR=../murmer_client/build cargo run
```

## Where the documentation is

| You want to | Read |
| --- | --- |
| Understand how the system fits together | [`docs/architecture.md`](docs/architecture.md) |
| Know the conventions and constraints | [`AGENTS.md`](AGENTS.md) |
| Do a specific kind of work | the task guides in [`agents/skills/`](agents/skills/), indexed in `AGENTS.md` |
| Run or write tests | [`docs/testing.md`](docs/testing.md) |
| Deploy or configure a server | [`README.md`](README.md) |
| Write a bot | [`murmer_server/BOT_API.md`](murmer_server/BOT_API.md) |

`AGENTS.md` is written for both humans and AI coding agents; `CLAUDE.md`
files throughout the repo are one-line pointers to it, so there is only ever
one set of instructions to keep current.

## Before you open a pull request

Run the same checks CI runs:

```bash
cd murmer_server && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

```bash
cd murmer_client && bun run check && bun run test
```

```bash
cd murmer_client/src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings
```

`bun run check` must report **0 errors and 0 warnings**.

CI sweeps the lockfiles for security advisories once a week
(`.github/workflows/audit.yml`). The same two checks by hand, worth running
when you change a dependency rather than waiting for the sweep:

```bash
cd murmer_server && cargo audit
```

```bash
cd murmer_client && bun audit
```

And two things no command can do for you:

- **Look at the change in the running app** if it has any visible effect.
  There are no component rendering tests here on purpose —
  [`agents/skills/visual-verification.md`](agents/skills/visual-verification.md)
  explains how to drive the app.
- **Smoke-test networking, authentication and file handling** by hand. The
  reconnect and auth-failure paths are not covered by the suites.

## What makes a change easy to merge

- **One coherent change per pull request**, and per commit. Unrelated fixes
  bundled together are the most common reason review stalls.
- **Say what you verified**, not just what you changed. "Checked the member
  list in both themes with a nickname set" is worth more than a paragraph of
  description.
- **Update the documentation you invalidated.** A change that makes
  `README.md` or a `docs/` page wrong is not finished. Documentation lives
  close to the thing it describes; `AGENTS.md` explains which tree gets what.
- **Keep it simple.** Prefer the flatter folder, the smaller component, the
  plain function. A pattern that pays off at ten contributors costs at two.
- **Don't add compatibility shims.** Only the latest version of everything is
  supported. The one exception is the identity backup format, which is a
  promise to anyone holding an old backup — see
  [`agents/skills/crypto-changes.md`](agents/skills/crypto-changes.md).

## Things that will be sent back

- **A permission check on the client only.** The server is the single
  enforcement point; client gating is cosmetic.
- **A plaintext path for encrypted content.** DMs and end-to-end encrypted
  channels must stay opaque to the server.
- **A hand-edited version number.** Use `bun run bump` —
  [`agents/skills/releasing.md`](agents/skills/releasing.md).
- **A TypeScript 7 upgrade**, including the Dependabot PR that proposes it.
  `svelte-check` is the blocker; the client stays on major 6.
- **A second copy of a list that already has an authority** — environment
  variables, permission flags, upload categories. Link to the authority, or
  add it to the mirror test
  ([`agents/skills/mirrored-constants.md`](agents/skills/mirrored-constants.md)).

## Branches and releases

Feature branches merge into `dev`; `main` is the release branch. Dependabot
targets `dev`. Releases are cut from a version tag — see
[`agents/skills/releasing.md`](agents/skills/releasing.md).

## Reporting bugs

Open an issue at <https://github.com/Tenosiey/Murmer/issues> with what you
expected, what happened, and the steps to reproduce it. Include the Murmer
version (Settings → About), your OS, and whether you were on the desktop app
or in a browser.

For anything voice-related, say whether the other peer was on the same
network — Murmer has no TURN relay yet, so two peers behind symmetric NATs
cannot connect at all. That is a known gap, not a bug:
[`plans/turn-support.md`](plans/turn-support.md).

Please **do not** open a public issue for a security problem. Email the
maintainer instead.
