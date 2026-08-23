# Screen sharing

`murmer_client/src/lib/screenshare/` plus the window layer in
`stores/screenShareWindows.ts`. It shares the repair policy in
`src/lib/webrtc/` with voice ([`voice.md`](voice.md)) but differs from it in
three ways that drive most of the design: several people may share at once,
a share may carry system audio, and a share is a *window on the user's
screen* rather than a row in a list.

## Transceivers: always offer audio

A share may carry system audio, so the viewer offers **recvonly video *and*
audio** transceivers. An answer can only fill m-lines the offer already
contains — without the audio one, a sharer with audio would have nowhere to
put its track.

Silent shares answer that m-line `inactive`, and that is how the viewer knows
whether to show its volume and mute controls. Check `currentDirection`, not
`getReceivers()`: every transceiver owns a receiver track, including the ones
carrying nothing.

## Keying: direction *and* peer

Everyone in a voice channel may share at the same time and watch each other,
so connections are keyed by **direction as well as peer name** — `incoming`
per sharer, `outgoing` per viewer — and every signaling frame carries the
sender's `role`.

One map keyed by name alone would hand a mutual pair of sharers a single
connection for two sessions. For the same reason, connections are repaired
under a `role:peer` key.

Stopping your own share therefore only closes `outgoing`; the shares you are
watching keep running.

Offers, answers and candidates must keep carrying `target`. The server routes
them to that peer alone instead of broadcasting, so a frame without one is
dropped and its session never connects. The client-side `target` checks stay
as they are — they are what makes the two ends agree on who a frame was for.
See [`protocol.md`](protocol.md).

## The viewer: a layer of floating windows

Watching never blocks the app. `ScreenShareLayer`/`ScreenShareWindow` render
over `watchedScreenShares` in `stores/screenShare.ts`; the layer is
`pointer-events: none` and only the windows themselves take input.

Each window is dragged, resized from any corner, shrunk into a corner
(picture-in-picture), maximized or made fullscreen on its own, and carries
its own volume/mute overriding the app-wide `screenShareVolume`/
`screenShareMuted` default. The sharer's self-preview is one of these windows
and starts in picture-in-picture.

Geometry lives in `stores/screenShareWindows.ts`, which owns every clamping
rule — **a window may never leave the viewport**, because a header dragged
off screen can never be grabbed again — and is handed the viewport by the
layer instead of reading `window`, so all of it is testable. Layouts persist
per sharer, namespaced by server URL like the other per-name state
([`client-state.md`](client-state.md)).

## Reconciliation: what says a share ended

A watched entry with no peer yet is still negotiating and stays up; one whose
peer disappears has ended and is closed. That is what keeps a window from
hanging on "Connecting…" forever.

It is also why a share **under repair keeps being reported** by
`getPeersList` even while it has no connection at all: dropping out of the
peer list is the store's signal that a share ended, so a repair that stopped
reporting would close the window it was trying to save.

The retained entry in `remoteStreams` is what says a share is still ours.
Every real teardown deletes it; `discardIncoming` (rebuild only) deliberately
does not, and the window keeps its last frame under a "Reconnecting…" overlay
meanwhile.

## Where repair differs from voice

- **A watched share gives up after `MAX_REBUILDS` and closes.** Voice repairs
  indefinitely, because a row in a member list can wait forever — a window on
  the user's screen cannot.
- **Offers only ever travel viewer → sharer**, so the viewer is always the
  side that re-offers. The sharer drops its outgoing connection and waits.

The screen-share bitrate cap is a server setting (Server Dashboard).
