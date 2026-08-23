# Server tests

Read this before writing an integration test in `murmer_server/tests/`.

The suite map is [`../../docs/testing.md`](../../docs/testing.md). This is how
to write one.

## One file per domain, one binary per file

Tests live in `murmer_server/tests/`, named for the area they cover
(`wiki_test.rs`, `roles_test.rs`, `security_limits.rs`, …).

**Every file becomes its own test binary. Do not merge files to save link
time.** `security_limits.rs` mutates process-wide environment variables with
`temp_env`, and separate binaries are what currently keeps that isolated from
the tests that build a `RateLimiter`.

The `[profile.dev]` settings in `Cargo.toml` exist for the same reason: full
DWARF made each of those binaries well over a hundred megabytes.

## The setup pattern

Run against a real in-memory database:

```rust
let db = db::init(":memory:").await.expect("in-memory db");
```

Give the file a helper that builds the fixtures it needs, the way
`tests/wiki_test.rs` does with its `setup` / `create` / `save` trio. Repeating
a twelve-argument call in every test is what makes a suite unmaintainable.

## Open with a doc comment

State why the area is worth testing at all. `tests/wiki_test.rs` is the
model — it names the paths where a mistake silently loses somebody's writing
and notes that none of them are visible in a smoke test. A reader deciding
whether to extend the file needs that more than they need the test names.

## What to assert

- **The denial, not just the happy path.** For anything authorization-shaped,
  the refusal is the assertion worth having: the wrong-role case, the
  equal-rank case, the off-roster case.
- **The invariants that carry a feature's security.** `channel_keys_test.rs`
  is the example — it pins the three rules in `db::insert_channel_keys` that
  the entire encrypted-channel design rests on.
- **Idempotence, where it is claimed.** Run a migration twice, and once
  against non-legacy state to prove it leaves user customization alone.
- **The invariant, not a snapshot**, so unrelated churn does not fail the
  file.

## Controlling time

Never sleep. The rate-limit window and the sweep interval are both a minute
long, so tests drive them through `RateLimiter::clock`:

```rust
let limiter = RateLimiter::with_clock(Clock::manual());
// ... then Clock::advance(...)
```

That is what lets `tests/security_limits.rs` reach behaviour a real sleep
never could — including the case that motivated it: *a sweep that silently
stopped running looks exactly like a working rate limiter.*

## Running

```bash
cd murmer_server && cargo test
```

`cargo test --test wiki_test` for a single file. `cargo fmt` and
`cargo clippy --all-targets -- -D warnings` must also pass clean — clippy
runs over test targets too.
