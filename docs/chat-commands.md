# Chat commands and history search

The Murmer chat input recognises several slash commands and exposes a search
overlay for looking through older messages. This guide walks through each
feature and explains how to trigger it from the desktop client.

## Slash commands

Type a forward slash `/` in the message box to access the commands below. The
client shows success or error feedback in the banner that appears directly above
the input field whenever a command runs. Normal messages continue to work even
if a command fails validation.

| Command | Description |
| --- | --- |
| `/help` | Opens an overlay listing every available slash command and its usage. |
| `/me <action>` | Sends an italicised third-person emote. |
| `/shrug [message]` | Appends the classic `¯\\_(ツ)_/¯` shrug to your text. |
| `/topic <text>` | Updates the current channel topic; send the command with no
message to clear the topic entirely. |
| `/status <online|away|busy|offline>` | Changes your presence indicator across
all clients. Invalid status values produce inline feedback with the list of
valid options. |
| `/focus` | Toggles focus mode (hides sidebars and other chrome for a compact
chat view). |
| `/ephemeral <seconds> <message>` or `/temp <seconds> <message>` | Sends a
message that is automatically deleted after the requested duration. Durations
are clamped between 5 seconds and 24 hours; the feedback banner confirms the
actual expiry time and warns when the value was adjusted. You must be signed in
before the client will send ephemeral messages. |
| `/search [initial query]` | Opens the search overlay and optionally pre-fills
the search box. Providing a query kicks off a search immediately. |

## Ephemeral message behaviour

Ephemeral messages display a countdown badge (for example, “Expires in 2m 30s”)
next to the message body, updating once per second. Hovering over the badge
reveals the exact expiry timestamp. The server re-validates every expiry window,
clamping it to the same 5 second – 24 hour bounds and scheduling automatic
cleanup once the timer elapses.

## History search overlay

Open the search interface from the magnifying glass button in the channel
actions bar or by running `/search`. The overlay keeps keyboard focus trapped
inside the dialog: type a query, press **Search** (or hit Enter), then review the
results list below. Each result shows a snippet, author, timestamp, and an
ephemeral badge when applicable. Click a result to scroll the main chat view to
that message. Use the **Close** button, press <kbd>Esc</kbd>, or click the shaded
background to dismiss the overlay.

Searches run against the currently selected channel and return up to 50 messages
per request. Blank queries are rejected with inline feedback, and network or
server errors surface as messages inside the dialog so you can retry.
