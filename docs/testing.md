# Testing

How the suites are shaped, what each one owns, and — the part that matters
most here — what deliberately is **not** tested. To write a test, follow
[`../agents/skills/client-tests.md`](../agents/skills/client-tests.md) or
[`../agents/skills/server-tests.md`](../agents/skills/server-tests.md).

## Suite map

| Runner | Where | Owns |
| --- | --- | --- |
| `bun run test` (Vitest) | `murmer_client/` | Pure client logic and store behaviour |
| `bun run check` (svelte-check) | `murmer_client/` | TypeScript + Svelte diagnostics; must be 0/0 |
| `cargo test` | `murmer_server/` | Integration tests against a real in-memory database |
| `cargo clippy --all-targets -- -D warnings` | both Rust crates | Lint, as a gate |

`.github/workflows/ci.yml` runs all of them in two parallel jobs on every
push to `main`/`dev` and every pull request. `cargo audit` and `bun audit`
are **not** part of CI — run them locally.

## The testing philosophy

Test the logic whose failure is **invisible**. That is the whole selection
rule, and it explains both what is covered and what is not.

Things that qualify, and are covered:

- per-server namespacing of persisted state (unread markers, window layouts);
- request/response correlation (the wiki store);
- parsing of untrusted server frames;
- policy that only manifests later — the channel-key rotation, the rate
  limiter's sweep, the VAD's noise-floor tracker;
- edge cases in pure algorithms — the wiki line diff, SDP munging, the
  recovery state machine.

Things that do not, and are deliberately absent:

- **Component rendering tests.** Do not add them. A broken layout is visible
  the moment you look at the app; see
  [`../agents/skills/visual-verification.md`](../agents/skills/visual-verification.md).
- **Reconnect and authentication-failure flows.** Manual smoke tests.
- **Anything needing a microphone or screen capture.** Not reachable from a
  test environment.

## Client suite

Unit tests sit next to their module as `src/lib/**/*.test.ts`. The shared
harness is in `test/`.

They run in a **plain Node environment** (`vitest.config.ts`) and stub only
what the stores need from the framework:

- `test/setup.ts` — a minimal in-memory `Storage`, cleared before every test
  so persisted state cannot leak between them. A real DOM implementation
  would buy nothing but install time.
- `test/stubs/app-environment.ts` — the `$app/environment` stand-in.
  `browser` is `true`, because the app only ever runs in a browser (static
  adapter, SSR off) and stores guard `localStorage` access with it; `false`
  here would silently skip every persistence path under test.

Only two aliases are configured, `$lib` and `$app/environment` — that is
everything the app modules ask of the framework. `vitest.config.ts` is
deliberately *not* an extension of `vite.config.ts`: the SvelteKit plugin
wants a synced `.svelte-kit/` and a dev server, neither of which the store
tests need.

### The jsdom exception

Two suites opt out of the Node default with a `// @vitest-environment jsdom`
docblock, because both cover the message-HTML boundary and need a DOM to
parse into: `src/lib/markdown.test.ts` (DOMPurify) and `src/lib/emoji.test.ts`
(`emojifyHtml`'s tree walk).

**Use jsdom, not happy-dom.** happy-dom 20 mis-drives DOMPurify's tree walk
and leaves `<script>` tags in the output, so the suite would have asserted
the exact opposite of the truth.

### The mirror test

`test/server-mirror.test.ts` parses `murmer_server/src/` as **text** and
fails when the client's copies of the server's tables drift — permissions,
upload categories, chat-settings bounds, voice defaults.

Reading the Rust as text rather than executing it keeps this a plain Vitest
run with no toolchain of its own. The cost is that it is coupled to the
*formatting* of those files, which is handled deliberately: if a rewrite
makes a regex stop matching, the parse assertions fail loudly instead of
silently comparing nothing.

See [`../agents/skills/mirrored-constants.md`](../agents/skills/mirrored-constants.md).

## Server suite

Integration tests live in `murmer_server/tests/`, one file per domain, each
running against `db::init(":memory:")`.

**Every file in `tests/` becomes its own test binary.** Do not merge them to
save link time: `security_limits.rs` mutates process-wide environment
variables with `temp_env`, and separate binaries are what currently keeps
that isolated from the tests that build a `RateLimiter`.

Debug info is the reason for the `[profile.dev]` settings in `Cargo.toml`.
Full DWARF made each of those binaries well over a hundred megabytes;
`debug = "line-tables-only"` keeps file:line in panics and backtraces at a
fraction of the link time and disk, and `split-debuginfo = "unpacked"` leaves
the rest in separate files rather than copying it into every binary.

### Controlling time

`RateLimiter::clock` exists so tests do not sleep. The rate-limit window and
the sweep interval are both a minute long, so
`RateLimiter::with_clock(Clock::manual())` plus `Clock::advance` is what lets
`tests/security_limits.rs` reach behaviour a real sleep never could.

## Conventions worth keeping

- **Name the invariant, not the snapshot.** Assert the property the test is
  named for, so unrelated churn does not fail it.
- **Open each test file with a doc comment saying why the area is worth
  testing.** `tests/wiki_test.rs` is the model: it names the paths where a
  mistake silently loses somebody's writing.
- **Prove idempotence where it is claimed.** A migration test runs the
  migration twice, and once against non-legacy state to prove it leaves user
  customization alone.
- **Prefer a pure function.** If a piece of logic is hard to test, the usual
  reason is that it is reaching for a store, `window` or the WebSocket.
  `webrtc/recovery.ts` and `wiki/diff.ts` are the pattern: no ambient
  dependencies, so the timings and edge cases are testable directly.
