# Releasing

Read this before bumping a version or cutting a release.

**Never edit a version by hand.** Six files carry it and they must agree;
`--locked` builds fail when the lock files disagree with their manifests.

## The scheme

Date-based: `YYYY.MDD.N` — year, month+day, and a counter for multiple
releases on the same day. `2026.710.0` is the first release on 2026-07-10.

It stays semver-ordered, which is not cosmetic: the Tauri updater only offers
an update when the new version compares **greater** than the installed one.

**Client and server share one version** and are bumped in lockstep. The
server crate must never be bumped separately or skipped.

## Bumping

```bash
cd murmer_client && bun run bump
```

`scripts/bump-version.mjs` computes the next version and writes it into all
six versioned files:

| File | Why it matters |
| --- | --- |
| `murmer_client/package.json` | the client's own version |
| `murmer_client/src-tauri/tauri.conf.json` | what the updater compares |
| `murmer_client/src-tauri/Cargo.toml` | the shell crate |
| `murmer_client/src-tauri/Cargo.lock` | `--locked` builds fail if it lags |
| `murmer_server/Cargo.toml` | the server crate |
| `murmer_server/Cargo.lock` | same reason as above |

`bun.lock` needs no bump — it does not record the root version.

## Tagging

```bash
git commit -am "Release v<version>"
```

```bash
git tag v<version>
```

```bash
git push origin v<version>
```

Pushing the tag triggers `.github/workflows/release.yml`, which builds the
NSIS installer, signs the updater artifacts and publishes a regular GitHub
release.

**The release must not be marked pre-release.** The updater endpoint
`releases/latest/download/latest.json` ignores prereleases, so a prerelease
publishes artifacts nobody receives.

## Before you tag

Run the full quality gate — a tag is the one action here that reaches users:

```bash
cd murmer_server && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

```bash
cd murmer_client && bun run check && bun run test && bun run build
```

## The signing key

Updater artifacts are signed with a keypair generated once
(`bun run tauri signer generate`). The public half lives in
`plugins.updater.pubkey` in `src-tauri/tauri.conf.json`; the private half and
its password are the `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repository secrets.

**If the private key is lost, every existing install stops receiving updates
and users must reinstall manually.** Full setup instructions are in the
Releases section of [`../../README.md`](../../README.md).
