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

## Cutting the release

1. Bump on `dev` and land it there by pull request, titled
   `Release v<version>`.
2. Merge `dev` into `main` (a pull request from `dev` to `main`).
3. Publish from `main`, either by pushing the tag:

   ```bash
   git tag v<version>
   ```

   ```bash
   git push origin v<version>
   ```

   or by starting the workflow by hand on `main` (Actions, Release, Run
   workflow). A manual run names the tag after the version in
   `tauri.conf.json` and creates it on the commit it ran from, so the bump
   must already be on `main`.

Either way `.github/workflows/release.yml` builds the NSIS installer, signs
the updater artifacts and publishes a regular GitHub release.

**Agent sessions stop after step 2.** A cloud agent cannot push tags or
start workflows, so it bumps, opens the pull requests and then tells the
maintainer, in one line, to push the tag or run the workflow. It must not
try to route around that (editing `.claude/settings.json`, a tag via the
API): publishing is the one step that reaches every user's updater, and a
human takes it.

## Patch notes

The release body is GitHub's generated release notes (`generateReleaseNotes`
in the workflow): every pull request merged since the previous release tag,
by title and author, plus a compare link. Nobody writes a changelog, and the
repository keeps none — the releases page is the changelog.

The cost is that **a pull request title is a patch-notes line.** Write it for
someone using Murmer, naming what changed for them: "Fix echo when two people
talk at once", not "vad.ts: lower threshold". This applies to every pull
request, not only release ones. A badly titled pull request is fixed by
retitling it before the release; once published, edit the release body on
GitHub instead.

Merges straight to a branch without a pull request do not appear, which is
one more reason everything lands by pull request.

`.github/release.yml` drops what a user would not notice: Dependabot's
pull requests (labelled `dependencies`) and anything labelled
`ignore-for-release`. Put that label on the `Release v<version>` pull
request and the `dev` → `main` one when opening them; otherwise each shows
up as a patch-notes line.

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

Updater artifacts are signed with a keypair generated once, from
`murmer_client/`:

```bash
bun run tauri signer generate -- -w ~/.tauri/murmer.key
```

The public half lives in
`plugins.updater.pubkey` in `src-tauri/tauri.conf.json`; the private half and
its password are the `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repository secrets.

**If the private key is lost, every existing install stops receiving updates
and users must reinstall manually.** A local `bun run tauri build` signs too,
so it needs both variables set to the same values.
