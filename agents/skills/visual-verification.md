# Visual verification

Read this before finishing any change with a visible effect: layout, styling,
overlays, menus, the chat surface, the Server Dashboard, notifications,
the invite flow, screen-share windows.

There are deliberately **no component rendering tests** in this repo. A
broken layout is obvious the moment you look at the app and expensive to
assert in a test, so the verification step is looking at the app. Skipping it
means nothing checked the change at all.

Inspect for clipping, overlap, wrong spacing, stale state, focus problems and
theme regressions — in **both** light and dark — before calling the work
done.

## Running the app

The desktop shell:

```bash
cd murmer_client && bun run tauri dev
```

The web client against a local server, which is the faster loop and the only
one an agent can drive:

```bash
cd murmer_server && cargo run
```

```bash
cd murmer_client && bun run dev
```

## Driving it from an agent session

The most reliable setup is to let the **server serve the client**, so the
page and `/ws` share an origin and no proxy or CORS juggling is needed:

```bash
cd murmer_client && bun run build
```

```bash
cd murmer_server && WEB_CLIENT_DIR=../murmer_client/build cargo run
```

Then open that one port in the browser pane. Deep links work, so
`/invite#url=…&name=…` exercises the whole invite flow.

For a throwaway instance that cannot touch your real data:

```bash
cd murmer_server && DATABASE_PATH=/tmp/scratch.db BIND_ADDRESS=127.0.0.1:3999 cargo run
```

### Things that will otherwise waste an hour

- **Synthetic `Enter` does not submit a form** in an automated browser pane.
  Drive forms with `form.requestSubmit()`.
- **Svelte inputs need the native value setter plus a bubbling `input`
  event.** Setting `.value` alone does not update the binding.
- **Microphone and screen capture are blocked**, so voice join — and
  everything gated on `inVoice`, such as the screen-share controls and the
  soundboard panel — cannot be exercised through the UI. Build the state with
  scripted users instead.
- **Voice-scoped *server* features are fully scriptable** even though the UI
  is not: `voice-join` is pure server state with no microphone involved. Two
  scripted users can join a voice channel and assert on the channel-scoped
  broadcasts.
- **Screenshots may be unavailable** if the pane has no compositor. Reading
  the page (accessibility tree, text, evaluated JS) still works — drive and
  assert through those.
- **Vite dev serves source modules**, so a store can be inspected in-page
  with `await import('/src/lib/stores/<store>.ts')` plus a subscribe /
  unsubscribe round trip.

### Simulating other users

Node has a global `WebSocket`. A scripted peer authenticates with a
`presence` frame: `{type:'presence', user, publicKey, signature, timestamp}`,
where `timestamp` is epoch-millis **as a string** and `signature` is
`nacl.sign.detached` over it.

Two things bite:

- **Names bind permanently to the first key that claims them.** Use fresh
  usernames per run, or persist the scripted users' keypairs.
- **Raise `MAX_AUTH_ATTEMPTS_PER_MINUTE`** for the scratch server; the
  default trips after a handful of connects from `127.0.0.1`.

To reach moderator-gated UI, bootstrap an Owner through `/role` with
`ADMIN_TOKEN` set on the scratch server. The body is `{key, role}` — the
base64 public key under `key`, **not** `publicKey` — with
`Authorization: Bearer <ADMIN_TOKEN>`. The browser user's key is in
`localStorage.murmer_keypair`.

## Push-to-talk

Push-to-talk cannot be verified in a browser at all. On Windows, check it
with Murmer focused **and** with another application in front — the latter
only applies to combos with a modifier or function key, since a bare key is
never grabbed system-wide.

## Before finishing

State what you actually looked at. "Verified in the running app" with no
detail is not a verification; naming the surface and the states you exercised
is.
