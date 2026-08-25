# macOS client — design note

**Status:** not implemented, not scheduled. Written 2026-08-25 as the plan for
shipping a Mac client that installs as easily as the Windows one and is built
and attached to the GitHub release by the same tag-triggered workflow.

The client itself is already portable — this is almost entirely a packaging,
signing and CI problem, plus three macOS-specific things that will silently
break voice if they are missed.

---

## What "as simple as the Windows client" actually means

Today a Windows user downloads `Murmer_<version>_x64-setup.exe` from the
release page, double-clicks it, clicks through SmartScreen's "unknown
publisher" warning, and is done. The Mac equivalent is a `.dmg` that opens to
the app next to an `Applications` alias — drag, done.

The catch is that **Gatekeeper is not SmartScreen**. SmartScreen puts a
"Run anyway" button in the warning itself. Gatekeeper, for a downloaded (and
therefore quarantined) app that is not signed with a Developer ID *and*
notarized, refuses with *"Murmer cannot be opened because Apple cannot check
it for malicious software"* and offers only "Move to Trash" / "Cancel". The
override lives somewhere else entirely: System Settings → Privacy & Security,
scroll to the bottom, "Open Anyway", confirm, admin password. macOS Sequoia
removed the older right-click → Open shortcut, so on any current macOS that
Settings detour is the only route.

It is a **one-time** step per install, not per launch, and the updater does
not re-trigger it — quarantine is applied by the browser that downloaded the
DMG, not by the app updating itself. So the install is "possible with an extra
step", not "impossible". The recurring cost is the microphone, below.

---

## Two decisions to make first

### A. Sign and notarize, or ship unsigned?

| | Apple Developer Program | Unsigned |
| --- | --- | --- |
| Cost | $99/year, recurring | nothing |
| Install | double-click, drag, done | + Settings detour, once |
| Mic permission | granted once, survives updates | breaks on every update |
| Updater | works silently | installs fine, then mic is dead |
| CI setup | 6 secrets, cert export | nothing |

The microphone row is the one that is easy to overlook, and it is the reason
to take decision A seriously rather than defaulting to free.

macOS TCC keys a granted permission to the app's **code signing requirement**,
not to its name or path. A Developer ID gives that requirement a stable team
identifier, so a grant survives every future version. An ad-hoc signature —
what an unsigned build gets, because arm64 binaries must carry *some*
signature to run at all — has no team identifier, so TCC falls back to the
code digest (cdhash), which changes with every single build.

The failure mode this produces is worse than a repeated prompt. The stored
requirement no longer matches the updated binary, so the microphone is denied
— but the *row* in Privacy & Security survives, toggle still switched on. The
user sees "Murmer ✓ Microphone" and has no microphone, with no obvious way to
re-grant short of deleting the entry or the app. Reported repeatedly against
other ad-hoc signed apps; it is a property of TCC, not of any one framework.

For a voice chat app that ships auto-updates, that is a bug report after every
release, and one that does not look like a permissions problem to the person
hitting it.

**Recommendation:** pay for the Developer Program if the Mac client is meant
for anyone other than you. If it is not worth $99/year, ship unsigned but be
honest about it in the README and do not describe it as "as simple as Windows"
— it is one extra step forever.

Everything below is written so that the unsigned path is the same work minus
the signing secrets; the decision can be deferred until step 4, but not past
it.

### B. Universal binary, or one DMG per architecture?

**Recommendation: universal** (`--target universal-apple-darwin`). One asset
on the release page, no "which one do I need" question, and no way for a user
on Rosetta to pick wrong. Costs roughly double the bundle size and one extra
compile of the whole tree per release; on a free public-repo macOS runner that
is minutes, not money.

`tauri-action` already handles the updater side of this: for a universal build
it writes **both** `darwin-aarch64` and `darwin-x86_64` entries into
`latest.json`, both pointing at the same archive, so installed clients on
either architecture find an update.

---

## What already works, and what is actually missing

Portable today, no change needed:

- `src-tauri/icons/icon.icns` exists and is already listed in `bundle.icon`.
- `src/lib.rs` has exactly one platform block, `#[cfg(target_os = "linux")]`
  for the WebKitGTK/appindicator workarounds; `main.rs`'s
  `windows_subsystem = "windows"` is already `cfg_attr`-gated.
- `scripts/run-tauri.js` invokes the Tauri CLI through `process.execPath`
  specifically so it behaves the same on all three platforms.
- The updater UI in `SettingsModal.svelte` already calls `relaunch()` after
  install with the comment "On Windows the installer exits the app itself;
  relaunch covers other platforms" — the non-Windows path was written for.
- `scripts/bump-version.mjs` names its six files explicitly, so adding a
  platform config file does not touch the release bump.

Genuinely missing:

1. `bundle.targets` is hardcoded to `["nsis"]`.
2. No `Info.plist` — **the app will be killed by TCC** the first time it asks
   for the microphone (see step 1).
3. No entitlements file, which the hardened runtime needs once notarization is
   on.
4. `release.yml` is `runs-on: windows-latest`, single job.
5. CI never compiles the shell for macOS, so a break would only surface at tag
   time, which is the worst possible moment.

---

## Step 1 — `src-tauri/Info.plist` (blocking; do this first)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>NSMicrophoneUsageDescription</key>
  <string>Murmer needs your microphone so you can talk in voice channels.</string>
  <key>NSCameraUsageDescription</key>
  <string>Murmer needs your camera for video in voice channels.</string>
</dict>
</plist>
```

Tauri merges a file at this exact path into the generated `Info.plist` on
macOS builds; on Windows and Linux it is ignored, so it costs nothing there.

This is not a nicety. On macOS, calling `getUserMedia` from a bundle with no
usage-description string does not reject the promise — the OS **terminates the
process**. `voice/capture.ts` catches `getUserMedia` failures carefully and
none of that code runs, because there is no process left. The symptom is "the
app vanishes when I join voice", with nothing in any log.

Screen recording has no usage-description key; it is a plain TCC prompt the
first time `getDisplayMedia` runs. See step 7 for whether it runs at all.

## Step 2 — `src-tauri/Entitlements.plist`

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>com.apple.security.device.audio-input</key>
  <true/>
  <key>com.apple.security.device.camera</key>
  <true/>
</dict>
</plist>
```

Notarization requires the hardened runtime, and under the hardened runtime the
usage string alone is not enough: without these entitlements the mic is denied
even after the user says yes. Harmless on the unsigned path, so add it either
way rather than making it conditional on decision A.

## Step 3 — `src-tauri/tauri.macos.conf.json`

```json
{
  "bundle": {
    "targets": ["app", "dmg"],
    "macOS": {
      "minimumSystemVersion": "13.0",
      "entitlements": "Entitlements.plist"
    }
  }
}
```

Tauri merges `tauri.<platform>.conf.json` automatically; `tauri.conf.json`
keeps `"targets": ["nsis"]` for Windows and needs no change. Note the merge is
RFC 7396: objects merge key by key but **arrays are replaced wholesale**,
which is exactly why `targets` has to be restated here in full rather
than extended.

Both targets are needed: `dmg` is what users download, `app` is what
`createUpdaterArtifacts` turns into `Murmer.app.tar.gz` + `.sig` for the
updater. Dropping `app` silently produces a release with no macOS update path.

`13.0` (Ventura) is the floor because that is where WebKit's `getDisplayMedia`
support starts, and it matches the project's "no backwards compatibility"
rule. The default would be 10.13.

## Step 4 — `release.yml`: draft → matrix → publish

The current single job becomes three. The middle one is a matrix; the outer
two exist so that two build jobs never race to create the same release.

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  create-release:
    runs-on: ubuntu-latest
    permissions:
      contents: write
    outputs:
      release_id: ${{ steps.create.outputs.result }}
    steps:
      - uses: actions/github-script@v8
        id: create
        with:
          script: |
            const tag = context.ref.replace('refs/tags/', '');
            const { data } = await github.rest.repos.createRelease({
              owner: context.repo.owner,
              repo: context.repo.repo,
              tag_name: tag,
              name: `Murmer ${tag}`,
              body: 'See the assets below to download and install this version.',
              draft: true,
              prerelease: false
            });
            return data.id;

  build:
    needs: create-release
    permissions:
      contents: write
    strategy:
      fail-fast: false
      matrix:
        include:
          - platform: windows-latest
            args: ''
          - platform: macos-latest
            args: '--target universal-apple-darwin'
    runs-on: ${{ matrix.platform }}
    steps:
      - uses: actions/checkout@v7
      - uses: oven-sh/setup-bun@v2
        with:
          bun-version: latest
      - uses: actions/cache@v6
        with:
          path: ~/.bun/install/cache
          key: ${{ runner.os }}-bun-${{ hashFiles('murmer_client/bun.lock') }}
          restore-keys: ${{ runner.os }}-bun-

      # `rustup show` installs the channel pinned in rust-toolchain.toml.
      # `rustup target add` then lands on that same pinned toolchain because
      # the directory override applies — see the note below.
      - name: Install pinned Rust toolchain
        run: rustup show
      - name: Add macOS cross targets
        if: matrix.platform == 'macos-latest'
        run: rustup target add aarch64-apple-darwin x86_64-apple-darwin

      - uses: swatinem/rust-cache@v2
        with:
          workspaces: murmer_client/src-tauri
      - name: Install frontend dependencies
        working-directory: murmer_client
        run: bun install --frozen-lockfile

      - name: Build and upload
        uses: tauri-apps/tauri-action@v1
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
          # Decision A. Omit this whole block on the unsigned path.
          APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}
          APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
          APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}
          APPLE_ID: ${{ secrets.APPLE_ID }}
          APPLE_PASSWORD: ${{ secrets.APPLE_PASSWORD }}
          APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}
        with:
          projectPath: murmer_client
          releaseId: ${{ needs.create-release.outputs.release_id }}
          args: ${{ matrix.args }}

  publish-release:
    needs: [create-release, build]
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/github-script@v8
        with:
          script: |
            await github.rest.repos.updateRelease({
              owner: context.repo.owner,
              repo: context.repo.repo,
              release_id: ${{ needs.create-release.outputs.release_id }},
              draft: false,
              prerelease: false
            });
```

Four things about this that are not obvious:

- **The toolchain pin bites here.** The current workflow uses
  `dtolnay/rust-toolchain@stable`, but `rust-toolchain.toml` pins `1.97.1`, so
  cargo runs 1.97.1 regardless. Adding the two Apple targets via that action's
  `targets:` input would install them into *stable* and the universal build
  would fail with "target may not be installed". Hence `rustup show` (matching
  what `ci.yml` already does) plus an explicit `rustup target add` from inside
  the checkout, where the directory override applies.
- **`latest.json` survives two jobs.** `tauri-action` reads the existing
  `latest.json` asset off the release before uploading and merges its
  `platforms` map, so whichever job finishes second keeps the first one's
  entry. This is why the draft release must be created up front and both jobs
  must be given the same `releaseId`.
- **The release must end non-draft and non-prerelease**, which is what
  `publish-release` is for. The updater endpoint
  `releases/latest/download/latest.json` skips both. This constraint is
  already documented in `agents/skills/releasing.md`; the draft window is fine
  because nothing points at a draft.
- **`fail-fast: false`** so a macOS failure does not throw away a finished
  Windows build. If one platform fails, the release stays a draft with partial
  assets — visible, fixable, and not something a user's updater ever sees.

## Step 5 — a macOS job in `ci.yml`

`release.yml` only runs on a tag, which means a macOS-only compile break would
be discovered at the exact moment it is most expensive. Add a third job that
does for macOS what the `client` job does for Linux — `bun run build`, then
`cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` in
`src-tauri` — minus the apt block, which macOS does not need (WebKit is part
of the OS). No bundling, no signing; this is a compile gate, not a build.

## Step 6 — verify on real hardware

The one part that cannot be done from CI or from a Linux machine. Per
`agents/skills/visual-verification.md`, anything with a visible effect gets
checked in the running app, and on a first-ever platform that is the whole
app. Checklist, in the order things break:

- [ ] DMG opens, drag-to-Applications works, app launches from Applications
      (not just from the mounted image — a quarantined app behaves
      differently)
- [ ] Microphone prompt appears **once**, voice connects, PTT works
- [ ] Global voice hotkeys fire while another app is focused
- [ ] Tray icon appears in the menu bar and its menu works — see the note
      below, this one is likely to need work
- [ ] Native notifications arrive
- [ ] Window position/size is restored across restarts (window-state plugin)
- [ ] File upload via the dialog plugin, and link previews
- [ ] **Updater end-to-end**: install the *previous* version, then update to
      this one. This is the step most likely to reveal a wrong platform key,
      and the only one that cannot be re-tested without cutting another tag
- [ ] **Microphone still works after that update** — on the unsigned path this
      is the one expected to fail, and it fails while Privacy & Security still
      shows the permission as granted. Confirm which way it goes before
      writing any README instructions about it
- [ ] Screen share — see step 7

**Menu bar icon.** `set_tray_theme` switches the tray between the light and
dark logo following the *app's* theme store. On macOS the menu bar follows the
*system* appearance, which the app theme need not match, and macOS expects a
monochrome template image (`isTemplate`) at roughly 22pt so it inverts itself.
The current 32px colour logo will likely look oversized and wrong in the wrong
appearance. Decide on hardware whether to ship a template variant for macOS or
accept it for a first release; do not guess it from here.

## Step 7 — screen sharing is the known risk

`screenshare/manager.ts` calls `getDisplayMedia`. On macOS that goes through
WKWebView, and the situation there is unresolved upstream: wry issue #1195
reports `getDisplayMedia` being denied because the `SCContentSharingPicker` is
never shown, and `getUserMedia` prompting twice on some macOS 14.x versions.
wry 0.56.0 (2026-07-30) shipped an expanded permission API that covers
`DisplayCapture`, but the pinned tree here is on wry 0.55.1 via tauri 2.11.5,
and no tauri release depends on 0.56 yet.

So: test it, and if it does not work, gate the share button on macOS with an
explicit "not supported on macOS yet" message rather than letting it fail
silently. That is cosmetic client-side gating of a client-side capability, not
a permission decision, so it does not conflict with the server-is-the-only-
enforcement-point invariant. Revisit when a tauri release picks up wry 0.56.

Voice itself does **not** depend on this — `getUserMedia` and the WebRTC peer
connections are a separate path and are expected to work once step 1 is in
place.

## Step 8 — hotkey conventions

`HOTKEY_ACTIONS` defaults to `Ctrl+Shift+M/O/V` and `Ctrl+F`, and the
in-app handlers check `ctrlKey` in two places. On macOS `Ctrl+F` is not what a
user reaches for; `Cmd+F` is.

Recommendation: **keep Ctrl for the three global voice hotkeys** — Cmd combos
are heavily reserved system-wide on macOS and grabbing them globally is
hostile — and consider platform-aware defaults only for the in-app actions
(`openSearch`, `openSettings`). That is a small change to `defaultBindings()`
plus the two `ctrlKey` checks, and it is optional for a first release: nothing
is broken without it, it is just slightly foreign. `comboToAccelerator`
already maps `Meta` → `Super`, which is what the plugin wants for Cmd.

## Step 9 — documentation

- `README.md` — a "macOS build instructions" section next to the Windows one,
  and, on the unsigned path, the Gatekeeper first-launch instructions in the
  download section. Environment variables and the release process are
  documented there and nowhere else; keep it that way.
- `agents/skills/releasing.md` — the tag now produces Windows *and* macOS
  artifacts, the release passes through a draft state, and a partial failure
  leaves it drafted.
- `docs/architecture.md` — the shell is no longer Windows-only.

---

## Suggested order

Steps 1–3 are one commit (packaging config; nothing observable changes on
Windows). Step 4 is one commit. Step 5 is one commit and can land first, since
it is the thing that tells you whether the shell even compiles for macOS.
Steps 6–8 depend on hardware and produce whatever commits they produce. Step 9
lands with the change that makes each document wrong.

The first tag after step 4 is the real test, and it is worth cutting a
throwaway `v2026.XXX.N` on a scratch repo or accepting one wasted version
number rather than discovering a broken `latest.json` on a release users will
actually receive.

## Open questions

- Decision A — is the $99/year worth it? Everything about the install and
  update experience hangs off this and nothing else in this note does.
- Is a Linux desktop bundle wanted in the same matrix? The shell already
  builds on Linux (CI proves it every push) and adding `ubuntu-latest` with
  the apt block would be nearly free at this point. Out of scope here, but
  this is the moment the workflow is being restructured anyway.
- Menu bar template icon: ship a monochrome macOS variant, or accept the
  coloured logo for the first release?
- Does the project want the mac client announced at all before screen sharing
  is confirmed working (step 7)?
