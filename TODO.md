# 📝 TODO List

An overview of planned work for the project.

Sections are ordered by what usually gets picked up first — bugs, then
hardening, then performance, then new work. Entries are sorted alphabetically
within each section.

Finished work is **removed** from this list rather than ticked off — git
history is the record of what shipped, and a list that only holds open items
stays readable. Use the checkboxes to mark something you have picked up.

Where an entry names a file or a symbol, that is the place to start reading,
not a description of the fix. Where an entry names another app, that is where
the idea comes from and what users will expect it to behave like.

---

## 🐛 Bugs

---

## 🔧 Tech debt / hardening

- [ ] Reclaim orphaned uploads. Deleting an emoji, avatar, server icon or
      sound removes its file; deleting a *message* does not, and neither does
      the Danger Zone purge or reset. The dashboard's storage breakdown can
      only ever grow. **Not** a sweep reconciling `uploads/` against the
      database: an encrypted channel seals its attachment URLs into `enc`, a
      forward into a DM seals them into the ciphertext, and the purge keeps
      DMs — so the server can never prove a file unreferenced, and a sweep
      would delete every attachment of every encrypted channel. The options
      that remain are the author's client naming the files when it deletes
      its own message, or an operator-chosen age limit for attachments
- [ ] TURN support — voice does not connect at all behind symmetric NAT or a
      network that blocks UDP. The ICE configuration already comes from the
      server (`STUN_SERVERS`, the `ice-config` frame); what is missing is the
      relay itself. Designed but not scheduled: see
      [`plans/turn-support.md`](plans/turn-support.md) for the work breakdown,
      the ephemeral-credential scheme, the interaction with
      `webrtc/recovery.ts` and the open questions

---

## ⚡ Performance

---

## 🚀 Features

### 🗨️ Chat Features

- [ ] Bulk delete for moderators — remove the last N messages of a channel,
      or everything one member posted in the last hour, with the ban
      (Discord's purge and "delete message history"). The Danger Zone only
      knows everything at once
- [ ] Outbound webhooks. The bot REST API covers "something else drives
      Murmer"; there is no way round for Murmer to notify something else when
      a message arrives
- [ ] Polls — a question with options, one vote per account, counts kept by
      the server (Discord, Teams). Refused in encrypted channels, as
      forwarding is: the server would have to see the votes to count them
- [ ] Saved messages — a personal bookmark list, separate from the
      server-wide pins
- [ ] Synced read state. Last-read ids live in `localStorage`
      (`stores/unread.ts`), so the desktop app and the web client each think
      the other's reading is still unread. Per-user read markers on the
      server fix it, DMs included
- [ ] Voice messages — record a clip in the composer and send it as an audio
      attachment (Skype, WhatsApp). Attachments already travel sealed in DMs
      and encrypted channels, so it inherits that for free

### 🎤 Voice Features

- [ ] AFK channel — move a member who has been deafened or silent for N
      minutes into a designated channel (TeamSpeak, Discord). In a full mesh
      an idle member still costs everyone a connection
- [ ] Direct calls — call someone from a DM, with ringing, accept and decline
      (Skype, Discord). Voice exists only in server channels today, so a
      private call means creating a private channel first. The peer
      connection code carries over; what is new is the ringing state and a
      call that belongs to no channel
- [ ] Move members between voice channels — drag a member onto another
      channel, behind a new `MOVE_MEMBERS` permission (TeamSpeak, Discord,
      Mumble). Breakout rooms already move people around, so the mechanism
      exists
- [ ] Poke — a short nudge that pops up even when the channel is muted
      (TeamSpeak). Rate-limited, and blockable with the member block
- [ ] Priority speaker — while a member with the permission talks, everyone
      else's playback is ducked (Mumble, Discord). The soundboard ducking
      (`voice/soundboard.ts`) is the same mechanism pointed at voices
- [ ] Server mute and deafen in voice (Discord, TeamSpeak). Like talk
      permission it can only be a hint, because audio is peer-to-peer — label
      it as such, and leave kicking from the channel as the hard option
- [ ] Temporary voice channels — a lobby that creates a personal channel on
      join and deletes it when the last person leaves (Discord "join to
      create", TeamSpeak temporary channels). Breakout rooms already have
      exactly that lifecycle
- [ ] Text chat scoped to a voice channel — somewhere to drop a link mid-call
      that does not interrupt the channel everyone else is reading
- [ ] Whisper — hold a hotkey to talk to selected members of the same
      channel only (Mumble, TeamSpeak). In the mesh that is muting the audio
      sender towards everyone else, so it needs no server change

### 🛠️ Other Features

- [ ] Accessibility pass — keyboard navigation and screen-reader labels.
      Context menus only open on right-click and take no arrow keys, so
      most of the app is currently hard to reach without a mouse
- [ ] Backup and export. For operators: a consistent snapshot of the database
      (`VACUUM INTO`) plus `uploads/` without stopping the server. For users:
      an export of their own DMs, which only their client can decrypt
- [ ] Narrow-window and touch layout. The web client is a shipped target, and
      nine `max-width` media queries in the whole client is what it has to
      meet a phone with
- [ ] Rules screening — new members accept the server rules before they can
      post (Discord membership screening). The welcome message already
      reaches first-time members; this makes it a gate
- [ ] Scheduled events — a time, a description, a voice channel and an RSVP
      list, with a reminder to everyone who said yes (Discord events). The
      reminder scheduler (`ws/handlers/scheduled.rs`) already runs
- [ ] Translatable UI. Every string is hardcoded English, so this is a
      structural change (extraction plus a lookup) rather than a translation
      job, and it only gets more expensive with every screen added

---

## 💡 Future Ideas

- [ ] An SFU for large voice channels — the real answer to the mesh's square
      growth, and a much bigger commitment than TURN: it puts media through
      the server, which today never sees any
- [ ] Background blur for the camera. Needs a segmentation model in the
      client — worth it once cameras are used routinely
- [ ] Call recording to a local file, with an indicator everyone in the
      channel sees (Mumble's recording notice). Recording without one is not
      an option
- [ ] Federation between Murmer servers (cross-server DMs)
- [ ] In-game overlay — a transparent always-on-top window showing who is
      talking (Discord, TeamSpeak overlays)
- [ ] LAN server discovery over mDNS, so a LAN party finds its server without
      anyone typing an address
- [ ] Local transcription and meeting notes. The server never sees media, so
      this can only run on a client (a local Whisper model in the desktop
      app), and only with the same visible indicator as recording
- [ ] Positional audio for games (Mumble)
- [ ] Screen-share annotations
