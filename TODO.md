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

- [ ] Drop the Google STUN default (`DEFAULT_STUN_SERVER` in
      `config.rs`). Every call by every user tells Google their IP and when
      they are in a call unless the operator changes it, which undoes the
      client deliberately having no default of its own (`iceConfig.ts`).
      Default to none, or to a server the project runs
- [ ] Encrypt attachment bytes in DMs and encrypted channels. Only the name
      and URL travel sealed today, so a DM'd photo or voice message sits
      in clear in `uploads/`, behind an unauthenticated `/files` URL, and is
      never deleted. Encrypting the file client-side with a key carried
      inside the sealed message fixes both, and is also what would let the
      orphaned-upload entry below delete them
- [ ] Erase one user's data. An erasure request (GDPR) today means
      hand-written SQL: the CLI has only `set-role` and `unbind-name`, the
      Danger Zone purges everything, retention skips DMs. A `murmer_server
      erase-user <name>` covering messages, DMs, files, reactions, profile
      and binding
- [ ] Forward secrecy for DMs. They are static NaCl `box` between long-term
      keys and kept forever, so whoever later gets a key reads every DM it
      ever received. A ratchet, or an established protocol rather than our
      own composition — through `agents/skills/crypto-changes.md`
- [ ] Identity key in the OS keychain (Tauri Stronghold or keyring) on
      desktop instead of `localStorage`. It is the root of the DM-history
      exposure above, and in the web client the operator serves the code
      that holds it
- [ ] Per-server identities derived from the one seed (HKDF of seed and
      server address). One key on every server lets any two operators, or a
      member of both, link a person's communities. Keeps the single backup;
      it is a crypto-format change, see `agents/skills/crypto-changes.md`
- [ ] Raid protection. On a password-less server anyone can mint unlimited
      keys and accounts; there is no join verification, account-age gate or
      captcha (Discord verification levels)
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

---

## 📚 Documentation

- [ ] Privacy at a glance in `README.md`, linked from every "encrypted" in
      the feature list: what the operator sees, that mesh voice shows each
      participant's IP to the others, that DM and encrypted-channel files
      are not encrypted, that DMs cannot be deleted, and that in the web
      client the operator serves the code holding your key
- [ ] A production deployment guide: a Caddy or nginx TLS front with
      `TRUSTED_PROXIES`, a password or invites, backing up the database and
      `uploads/`, and how long to keep logs (they pair names with IPs)

---

## ⚡ Performance

---

## 🚀 Features

### 🗨️ Chat Features

- [ ] Delete a member's recent messages with the ban — everything they
      posted in the last hour (Discord's "delete message history"). `/purge`
      covers the newest messages of one channel, whoever wrote them
- [ ] Delete and edit DMs. There is no frame for either, and
      `MESSAGE_RETENTION_DAYS` keeps DMs, so a DM is permanent. Delete
      removes the stored ciphertext for both sides
- [ ] Message reports — a member flags a message to the moderators, with a
      queue in the Server Dashboard (Discord's report to mods)
- [ ] Outbound webhooks. The bot REST API covers "something else drives
      Murmer"; there is no way round for Murmer to notify something else when
      a message arrives
- [ ] Saved messages — a personal bookmark list, separate from the
      server-wide pins

### 🎤 Voice Features

- [ ] AFK channel — move a member who has been deafened or silent for N
      minutes into a designated channel (TeamSpeak, Discord). In a full mesh
      an idle member still costs everyone a connection
- [ ] Always through the server — a per-user setting to take every call over
      the SFU, and an operator "SFU only" mode. In the mesh every
      participant learns everyone else's IP address, which Discord and
      TeamSpeak never reveal; today only `SFU_THRESHOLD=2` comes close
- [ ] Direct calls — call someone from a DM, with ringing, accept and decline
      (Skype, Discord). Voice exists only in server channels today, so a
      private call means creating a private channel first. The peer
      connection code carries over; what is new is the ringing state and a
      call that belongs to no channel
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
- [ ] Appear offline, and a setting to stop sending typing indicators
      (Discord's invisible status)
- [ ] Backup and export. For operators: a consistent snapshot of the database
      (`VACUUM INTO`) plus `uploads/` without stopping the server. For users:
      an export of their own DMs, which only their client can decrypt
- [ ] Discord import or bridge, so a community can move with its history or
      run both during a transition. Existing bots also need rewriting, since
      the bot API has its own shape
- [ ] Narrow-window and touch layout. The web client is a shipped target, and
      nine `max-width` media queries in the whole client is what it has to
      meet a phone with
- [ ] Release builds for macOS and Linux, and a signed Windows installer.
      `release.yml` builds Windows only and unsigned, and the people most
      likely to want Murmer are the ones trained to stop at SmartScreen's
      warning. macOS is `plans/macos-client.md`
- [ ] Rules screening — new members accept the server rules before they can
      post (Discord membership screening). The welcome message already
      reaches first-time members; this makes it a gate
- [ ] Scheduled events — a time, a description, a voice channel and an RSVP
      list, with a reminder to everyone who said yes (Discord events). The
      reminder scheduler (`ws/handlers/scheduled.rs`) already runs
- [ ] A second UI language. Every screen reads its text from the English
      catalog (`src/lib/i18n/`); what is missing is a second catalog and a
      language picker. The locale is picked once at startup, so the picker
      would reload the page

---

## 💡 Future Ideas

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
- [ ] Local transcription and meeting notes. The server sees no media in a
      mesh channel, so this can only run on a client (a local Whisper model
      in the desktop app), and only with the same visible indicator as
      recording
- [ ] Positional audio for games (Mumble)
- [ ] Screen-share annotations
- [ ] End-to-end encryption through the SFU (encoded transforms / SFrame),
      so a large call stays private from the operator too. Check
      `RTCRtpScriptTransform` in WebView2, WebKitGTK and WKWebView first
- [ ] ICE-TCP on the SFU port, for networks that block UDP entirely. str0m
      supports TCP candidates; nobody has asked yet
