# Database changes

Read this before changing the SQLite schema in `murmer_server/src/db/`.

There is **no migration directory and no schema version number.** The schema
is declarative and idempotent: `db::run_schema` runs on every startup, so a
schema addition reaches an existing database simply by being there. That is a
deliberate simplification, and it constrains how changes are made.

## Adding a table

Put `CREATE TABLE IF NOT EXISTS …` in `run_schema` (`db/mod.rs`), or in the
domain's own `*_schema()` function where one already exists —
`stats::stats_schema()`, `wiki::wiki_schema()`, `soundboard::soundboard_schema()`.
Follow whichever the surrounding domain uses.

## Adding a column

`CREATE TABLE IF NOT EXISTS` does **not** extend an existing table, so a new
column needs `ensure_column`:

```rust
ensure_column(conn, "user_keys", "nickname", "TEXT NOT NULL DEFAULT ''")?;
```

SQLite has no `ADD COLUMN IF NOT EXISTS`; `ensure_column` inspects
`pragma_table_info` first. Give the column a **`NOT NULL` default** so
existing rows are valid the moment it appears.

## One-time work: marker-guard it

Anything that must happen exactly once — a backfill, a permission grant to
existing roles, a destructive cleanup — is guarded by a marker row in
`server_settings`, and the guard and the work go in the **same transaction**.
The existing examples are the template:

- `roles::migrate_roles` — seeds built-in roles and folds legacy single-role
  assignments into `role_definitions`/`user_roles`.
- `roles::migrate_soundboard_permissions` and
  `migrate_nickname_permissions` — grant a new flag to the roles that should
  already have had it, so an existing server matches a freshly seeded one.
  Note what these deliberately do *not* do: `@everyone` never gains the
  nickname flag.
- The pre-E2EE direct-message wipe — old plaintext rows can neither be
  rendered by the client nor converted server-side, so they are deleted once
  and the `dm_e2ee` marker keeps it from ever running twice.

When you add a permission flag that existing servers should also have, decide
explicitly **which roles get it** and write that reasoning into the comment.
Silently granting it to `@everyone` is a privilege escalation on every
running server.

## Ordering inside `run_schema`

`run_schema` is a sequence, and some steps depend on earlier ones —
`migrate_roles` needs `server_settings`, created by `stats_schema` above it.
Add new work where its dependencies already exist, and say so in a comment if
the position is load-bearing.

## Indexes and derived tables

`messages_fts` is the cautionary tale worth copying. Whether the full-text
index needs backfilling is decided by whether the table existed **before**
this run created it: once it is there the triggers keep it current, so a
backfill is only ever owed by a database that predates the index. Creation
and backfill run in **one transaction**, which is what makes "the table
exists" and "the backfill completed" the same fact even across a crash.

The anti-join backfill scans every message row. It used to run on every
startup; do not reintroduce that shape.

## Queries

All queries run on **one connection thread**, so per-query cost is shared by
everything on the server. Use `prepare_cached`. The cache capacity is raised
in `db::init` because the default of 16 is below the number of distinct
statements here — if you add a batch of new statements, check that it is
still above the total.

## In-memory mirrors

Some rows are mirrored in `AppState` because every request consults them:
`chat_settings`, `channel_overrides`, `role_defs`, `user_roles`,
`voice_channels`, `stats_enabled`.

Two obligations come with adding one:

1. **Whatever writes the row refreshes the mirror in the same step.**
2. **Whatever deletes rows wholesale clears the mirror too.** The Danger Zone
   reset is the case that proves it: without clearing, a deleted channel
   stays joinable and a deleted role keeps granting permissions until the
   next restart.

## Tests

Add or extend a file in `murmer_server/tests/` — see
[`server-tests.md`](server-tests.md). For one-time work, prove **idempotence**
by running it twice, and prove it leaves non-legacy state alone by running it
against a fresh database.
