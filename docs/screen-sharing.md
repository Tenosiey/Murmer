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

The name and voice channel the manager signs its frames with are released
the moment nothing is shared or watched any more. They are adopted only while
none is held, and one that outlived its last share addressed every later
offer to the old channel — which the sharer drops as not meant for it, so the
window sat on "Connecting…" for good. Switching voice channels closes every
watched share for the same reason.

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

## Through the server (SFU)

In a channel the server has put in SFU mode
([`voice.md`](voice.md#through-the-server-sfu)) a share goes through the
server instead of the mesh. On the mesh the sharer
encodes and uploads once per viewer, which at 1080p and 8 Mbps to ten
viewers is 80 Mbps; through the SFU it is 8.

- **The sharer publishes once**, on one send-only connection to the server,
  sent as `sfu-screen-offer` with `sharer` set to themselves.
- **A viewer's offer goes to the server** (`sfu-screen-offer` naming the
  sharer) instead of to the sharer. Offers still only travel from whoever
  wants media, so the rest of this page applies unchanged. The server
  forwards the sharer's media to every viewer connection of that share.
- **Each connection keeps its route** until it is replaced, so a repair
  goes the way its connection was opened. A mode change moves every share
  at once, without voice's make-before-break: a watched share keeps its
  window and stream, shows "Reconnecting" for the moment the switch takes,
  and does not spend its rebuild budget.
- **The server only carries announced shares.** A share is forwarded while
  it is in `screenshare-start`/`-stop` state, and an offer for any other is
  dropped silently, as a refused mesh offer is.
- **The audio m-line is always answered.** The server cannot know whether
  the sharer will send audio, so a silent share shows its volume controls
  in SFU mode.

## Where repair differs from voice

- **A watched share gives up after `MAX_REBUILDS` and closes.** Voice repairs
  indefinitely, because a row in a member list can wait forever — a window on
  the user's screen cannot.
- **Offers only ever travel viewer → sharer**, so the viewer is always the
  side that re-offers. The sharer drops its outgoing connection and waits.

The screen-share bitrate cap is a server setting (Server Dashboard).
