# Murmer Feature Testing Checklist

This document lists all features in the Murmer app for manual testing. Check each feature to verify it works as expected.

Last updated: 2025-10-30

---

## 🔐 Authentication & Security

- [ ] **User Login** - Login with username works
- [ ] **Ed25519 Signature Authentication** - Authentication uses cryptographic signatures
- [ ] **Session Persistence** - Login session persists after closing and reopening app
- [ ] **Logout** - Logout clears session and returns to login screen
- [ ] **Server Password Protection** - Server with password requires correct password to connect
- [ ] **Rate Limiting Protection** - Server blocks excessive authentication attempts
- [ ] **Auto-reconnect** - App automatically reconnects after connection loss

---

## 💬 Chat Features

### Message Sending & Display
- [ ] **Send Text Messages** - Can send plain text messages in chat
- [ ] **Receive Messages** - Messages from other users appear in real-time
- [ ] **Message Timestamps** - Each message shows when it was sent
- [ ] **Message Grouping** - Messages from same user are visually grouped
- [ ] **Day Separators** - Messages are separated by day headers
- [ ] **Scroll to Bottom** - Can scroll to latest message
- [ ] **Auto-scroll on New Message** - New messages auto-scroll if already at bottom
- [ ] **Message History Loading** - Can load older messages by scrolling up
- [ ] **Message History Persistence** - Chat history is saved and loads on reconnect

### Markdown & Formatting
- [ ] **Markdown Formatting** - Messages support markdown (bold, italic, etc.)
- [ ] **Code Syntax Highlighting** - Code blocks have syntax highlighting
- [ ] **Code Block Language Detection** - Different languages highlight correctly
- [ ] **DOMPurify Sanitization** - HTML/scripts in messages are sanitized

### Message Actions
- [ ] **Delete Messages** - Can delete own messages
- [ ] **Edit Channel Topic** - Can set/clear channel topic via UI button
- [ ] **Message Context Menu** - Right-click on message shows options
- [ ] **Pin Messages** - Can pin important messages to channel
- [ ] **Unpin Messages** - Can remove pinned messages
- [ ] **View Pinned Messages** - Pinned messages panel shows all pins
- [ ] **Jump to Pinned Message** - Clicking pinned message scrolls to original
- [ ] **Message Search** - Can search chat history for text
- [ ] **Search Result Navigation** - Can click search results to jump to message
- [ ] **Message Highlighting** - Jumped-to messages are highlighted temporarily

### Reactions
- [ ] **Add Emoji Reactions** - Can react to messages with emoji
- [ ] **Remove Emoji Reactions** - Can remove own reactions by clicking again
- [ ] **View Reaction Count** - Reaction buttons show count of users
- [ ] **View Reaction Users** - Hovering reaction shows who reacted
- [ ] **Multiple Reactions per Message** - Multiple different emoji reactions work

### Special Messages
- [ ] **Ephemeral Messages** - Messages with expiration time auto-delete
- [ ] **Ephemeral Message Timer Display** - Shows countdown on ephemeral messages
- [ ] **Image Upload** - Can upload and send images
- [ ] **Image Display** - Uploaded images display in chat
- [ ] **Image Preview** - Click image to view full size
- [ ] **File Type Validation** - Non-image uploads are rejected
- [ ] **File Size Limits** - Oversized uploads are rejected
- [ ] **Filename Sanitization** - Malicious filenames are cleaned

### Slash Commands
- [ ] **`/help`** - Shows list of available slash commands
- [ ] **`/me <action>`** - Sends italicized third-person emote
- [ ] **`/shrug [message]`** - Appends shrug emoticon ¯\\\_(ツ)\_/¯
- [ ] **`/topic <text>`** - Sets channel topic
- [ ] **`/topic` (no args)** - Clears channel topic
- [ ] **`/status <online|away|busy|offline>`** - Changes user status
- [ ] **`/focus`** - Toggles focus mode (minimal UI)
- [ ] **`/ephemeral <seconds> <message>`** - Sends self-deleting message
- [ ] **`/temp <seconds> <message>`** - Alias for /ephemeral
- [ ] **`/search [query]`** - Opens search overlay with optional query
- [ ] **Slash Command Feedback** - Commands show success/error messages

### Mentions & Notifications
- [ ] **@Mentions** - Can mention users with @username
- [ ] **Mention Detection** - Mentions are highlighted/detected
- [ ] **Desktop Notifications** - Notifications appear for new messages
- [ ] **Mention Notifications** - Special notification when mentioned
- [ ] **Notification Sound** - (if implemented) Sound plays on notification
- [ ] **Per-Channel Notification Settings** - Can set all/mentions/mute per channel
  - [ ] All messages mode
  - [ ] Mentions only mode
  - [ ] Muted mode

---

## 🎤 Voice Chat Features

### Voice Channel Management
- [ ] **Join Voice Channel** - Can join voice channels
- [ ] **Leave Voice Channel** - Can leave voice channels
- [ ] **See Voice Channel Users** - Shows list of users in voice channel
- [ ] **Voice Join Sound** - Plays sound when user joins voice
- [ ] **Voice Leave Sound** - Plays sound when user leaves voice
- [ ] **Create Voice Channel** - Can create new voice channels
- [ ] **Delete Voice Channel** - Can delete voice channels (with permissions)
- [ ] **Update Voice Channel Settings** - Can configure voice channel bitrate/quality

### Voice Transmission
- [ ] **Voice Transmission** - Voice is transmitted to other users
- [ ] **Voice Reception** - Can hear other users in voice channel
- [ ] **Mute Microphone** - Can mute own microphone
- [ ] **Mute Output** - Can mute all incoming voice
- [ ] **Microphone State Persistence** - Mute state persists after restart

### Voice Modes
- [ ] **Continuous (Always On) Mode** - Voice transmits continuously
- [ ] **Voice Activity Detection (VAD)** - Voice activates on speaking
- [ ] **VAD Sensitivity Slider** - Can adjust VAD sensitivity
- [ ] **Push-to-Talk (PTT) Mode** - Voice transmits only when key pressed
- [ ] **PTT Key Configuration** - Can customize PTT key
- [ ] **PTT Key Capture** - Can capture any keyboard key for PTT
- [ ] **PTT Key Display Name** - Shows friendly name for PTT key (e.g., "Space")

### Voice Indicators
- [ ] **Voice Activity Indicator** - Shows when someone is speaking
- [ ] **Speaking User Highlight** - Speaking users are highlighted in list
- [ ] **Self Voice Activity Indicator** - Shows when you are transmitting
- [ ] **Connection Quality Bars** - Shows connection strength (5-bar display)

### Audio Settings
- [ ] **Master Volume Slider** - Controls overall output volume
- [ ] **Input Device Selection** - Can choose microphone
- [ ] **Output Device Selection** - Can choose speakers/headphones
- [ ] **Device Settings Persistence** - Audio device choices persist
- [ ] **Individual User Volume** - Can adjust volume per user (if implemented)

### WebRTC Features
- [ ] **Peer-to-Peer Connection** - WebRTC connections establish successfully
- [ ] **ICE Candidate Exchange** - ICE candidates are exchanged
- [ ] **Audio Codec Negotiation** - Audio codecs negotiate properly
- [ ] **Bitrate Configuration** - Audio bitrate can be configured
- [ ] **Connection Stats** - Voice connection statistics are tracked

---

## 🎨 UI/UX Features

### Theme & Layout
- [ ] **Dark Theme** - Dark theme works and looks good
- [ ] **Light Theme** - Light theme works and looks good
- [ ] **Theme Toggle** - Can switch between dark/light theme
- [ ] **Theme Persistence** - Theme choice persists after restart
- [ ] **System Theme Detection** - Uses system preference on first launch
- [ ] **Focus Mode** - Focus mode hides sidebars for distraction-free view
- [ ] **Focus Mode Toggle** - Can toggle focus mode on/off
- [ ] **Resizable Sidebars** - Can drag to resize channel and user sidebars
- [ ] **Sidebar Width Persistence** - Sidebar sizes persist after restart

### User Status
- [ ] **User Status Indicators** - Shows online/away/busy/offline status
- [ ] **Status Colors** - Different status types have distinct colors
- [ ] **Set Own Status** - Can change own status
- [ ] **Status Menu** - Status dropdown menu works
- [ ] **Status Sync** - Status updates appear for other users in real-time
- [ ] **Status Labels** - Status has friendly labels (e.g., "Online", "Away")
- [ ] **Status Emojis** - Status indicators have appropriate emoji

### Connection & Performance
- [ ] **Connection Status Display** - Shows if connected/disconnected
- [ ] **Ping Display** - Shows server latency (ping) in milliseconds
- [ ] **Connection Quality Indicator** - Visual bars show connection quality
- [ ] **Reconnection Indicator** - Shows when reconnecting
- [ ] **Connection Error Messages** - Clear error messages on connection failure

### Accessibility & Interaction
- [ ] **Keyboard Shortcuts** - Keyboard shortcuts work
  - [ ] **Ctrl+Shift+F** - Toggle focus mode
  - [ ] **Ctrl+Shift+M** - Toggle microphone mute
  - [ ] **Escape** - Close modals/overlays
- [ ] **Enter to Send** - Enter key sends message
- [ ] **Shift+Enter** - Shift+Enter adds new line in message
- [ ] **Auto-resize Input** - Message input grows with content
- [ ] **Input Scroll** - Message input scrolls when content exceeds max height
- [ ] **Context Menus** - Right-click context menus work
- [ ] **Click Outside to Close** - Menus close when clicking outside
- [ ] **Escape to Close** - ESC key closes modals/menus
- [ ] **Tab Navigation** - Can navigate UI with Tab key
- [ ] **Focus States** - Focus indicators are visible

---

## 📁 Channel Management

### Text Channels
- [ ] **View Channel List** - All text channels are visible in sidebar
- [ ] **Switch Channels** - Can switch between text channels
- [ ] **Active Channel Highlight** - Current channel is highlighted
- [ ] **Create Text Channel** - Can create new text channels
- [ ] **Delete Text Channel** - Can delete text channels (with permissions)
- [ ] **Channel Context Menu** - Right-click on channel shows options
- [ ] **Channel Topics** - Can set/view channel topics/descriptions
- [ ] **Channel Topic Display** - Topic appears above chat

### Channel Permissions
- [ ] **Admin-Only Channel Creation** - Only admins can create channels (if ADMIN_TOKEN set)
- [ ] **Admin-Only Channel Deletion** - Only admins can delete channels (if ADMIN_TOKEN set)
- [ ] **Role-Based Permissions** - Mod/Owner/Admin roles work correctly
- [ ] **Permission Error Messages** - Clear error when lacking permissions

### Channel Features
- [ ] **Per-Channel History** - Each channel has separate message history
- [ ] **Channel Switching Loads History** - Switching channels loads that channel's messages
- [ ] **Channel Notifications** - Notification settings work per-channel
- [ ] **Channel Persistence** - Channels persist on server restart

---

## 🖥️ Server Management

### Server List
- [ ] **Add Server** - Can add new server to list
- [ ] **Remove Server** - Can remove server from list
- [ ] **Edit Server** - Can edit server details (name, URL, password)
- [ ] **Server List Display** - All added servers shown on servers page
- [ ] **Server Persistence** - Server list persists in localStorage
- [ ] **Server Status Indicator** - Shows online/offline status per server
- [ ] **Select Server** - Can select/switch between servers

### Server Connection
- [ ] **Connect to Server** - Can connect to selected server
- [ ] **Server URL Validation** - Invalid server URLs are handled gracefully
- [ ] **Connection Timeout** - Connection timeout is handled properly
- [ ] **WebSocket Connection** - WebSocket connects successfully
- [ ] **WebSocket Reconnection** - Auto-reconnects on connection drop

### Server Invites
- [ ] **Generate Invite Link** - Can create murmer:// invite link
- [ ] **Copy Invite Link** - Can copy invite to clipboard
- [ ] **Parse Invite Link** - Can paste and parse murmer:// link
- [ ] **Add Server from Invite** - Server is added from invite link
- [ ] **Invite Includes Password** - Password is encoded in invite (if set)
- [ ] **Invite Link Format** - murmer://invite?url=... format works

### Server Features
- [ ] **Multiple Servers** - Can connect to multiple servers (switching between)
- [ ] **Server Roles** - User roles display with colored labels
- [ ] **Role Colors** - Role colors appear next to usernames
- [ ] **Admin Features** - Admin token enables permission system

---

## 🔔 Notifications

- [ ] **Desktop Notifications** - Desktop notifications appear (OS level)
- [ ] **Notification Permission Request** - App requests notification permission
- [ ] **Notification Content** - Shows sender and message preview
- [ ] **Mention Notifications** - Separate notification style for mentions
- [ ] **Notification Click** - Clicking notification focuses app (if implemented)
- [ ] **Per-Channel Notification Control** - Can mute specific channels
- [ ] **Notification Settings Persistence** - Notification preferences persist

---

## 🖼️ Media & Links

### Images
- [ ] **Image Upload Button** - Image upload button works
- [ ] **Image File Picker** - File picker opens for image selection
- [ ] **Image Preview Before Send** - Shows preview before uploading
- [ ] **Image Display in Chat** - Uploaded images display inline
- [ ] **Image Loading States** - Loading indicator while uploading
- [ ] **Image Error Handling** - Errors shown if upload fails

### Link Previews
- [ ] **Link Detection** - URLs in messages are detected
- [ ] **Generic Link Preview** - Web links show iframe preview
- [ ] **YouTube Link Preview** - YouTube videos show special preview
- [ ] **YouTube Thumbnail** - Shows video thumbnail
- [ ] **YouTube Metadata** - Shows video title and author
- [ ] **Link Preview Timeout** - Preview fails gracefully if site doesn't load
- [ ] **Link Preview Opens in Browser** - Clicking preview opens in external browser
- [ ] **Link Preview Fallback** - Shows fallback UI if preview unavailable

---

## ⚙️ Settings

### Settings Modal
- [ ] **Open Settings** - Settings modal opens
- [ ] **Close Settings** - Settings modal closes properly
- [ ] **Settings Persistence** - All settings persist after closing app

### Audio Settings
- [ ] **Volume Slider** - Master volume slider works
- [ ] **Volume Percentage Display** - Shows volume as percentage
- [ ] **Input Device Dropdown** - Lists all available microphones
- [ ] **Output Device Dropdown** - Lists all available speakers/headphones
- [ ] **Device Change** - Changing device switches audio I/O
- [ ] **Default Device Option** - "Default" option uses system default

### Voice Activation Settings
- [ ] **Voice Mode Dropdown** - Can select Always On / VAD / PTT
- [ ] **VAD Sensitivity Slider** - VAD sensitivity adjustment works
- [ ] **VAD Sensitivity Display** - Shows sensitivity as percentage
- [ ] **PTT Key Display** - Shows current PTT key
- [ ] **PTT Key Capture Button** - "Capture Key" button works
- [ ] **PTT Key Capture Process** - Pressing any key sets it as PTT key

### Update Settings
- [ ] **Check for Updates Button** - Update check button works
- [ ] **Update Check API Call** - Queries GitHub releases API
- [ ] **Update Available Message** - Shows message if update available
- [ ] **Already Updated Message** - Shows message if on latest version
- [ ] **Update Check Error Handling** - Handles API errors gracefully

---

## 🧪 Advanced Features

### Security Features
- [ ] **Replay Attack Protection** - Nonce system prevents replay attacks
- [ ] **Rate Limiting** - Message rate limiting works
- [ ] **Auth Rate Limiting** - Authentication attempts are rate limited
- [ ] **Filename Path Traversal Protection** - Malicious filenames are sanitized
- [ ] **Image Content-Type Validation** - Only image MIME types accepted
- [ ] **Admin Token Protection** - Admin endpoints require bearer token

### Developer Features
- [ ] **Version Display** - App version is visible in UI/settings
- [ ] **Console Logging** - Useful debug logs in browser console (dev mode)
- [ ] **Error Messages** - Clear error messages for debugging
- [ ] **WebSocket Message Logging** - WebSocket traffic logged (dev mode)

### Data Management
- [ ] **localStorage Usage** - Settings stored in localStorage work
- [ ] **Server-Side PostgreSQL Storage** - Messages stored in database
- [ ] **Message History Retrieval** - Can load messages from database
- [ ] **Search Database** - Search queries database correctly

---

## 🐛 Edge Cases & Error Handling

### Connection Issues
- [ ] **Server Offline** - Handles server being offline gracefully
- [ ] **Connection Loss During Use** - Handles unexpected disconnection
- [ ] **Reconnection After Network Loss** - Reconnects when network returns
- [ ] **Slow Connection** - Works acceptably on slow connection
- [ ] **Invalid Server URL** - Shows clear error for bad URLs

### Input Validation
- [ ] **Empty Message** - Cannot send empty messages
- [ ] **Whitespace-Only Message** - Cannot send whitespace-only messages
- [ ] **Very Long Messages** - Handles very long messages appropriately
- [ ] **Special Characters** - Special characters in messages work
- [ ] **Emoji in Messages** - Emoji in text messages work
- [ ] **Unicode Characters** - Non-ASCII characters display correctly

### Voice Edge Cases
- [ ] **No Microphone** - Handles missing microphone gracefully
- [ ] **Microphone Permission Denied** - Shows error if mic permission denied
- [ ] **Join Voice While Already in Voice** - Prevents double-joining
- [ ] **Voice Connection Failure** - Handles WebRTC connection failures
- [ ] **Voice Connection Timeout** - Handles ICE gathering timeout

### UI Edge Cases
- [ ] **Very Long Username** - Long usernames don't break layout
- [ ] **Very Long Channel Name** - Long channel names don't break layout
- [ ] **Many Channels** - UI works with many channels
- [ ] **Many Messages** - UI performs well with many messages
- [ ] **Rapid Message Sending** - Handles rapid message sending
- [ ] **Window Resize** - UI adapts to window resize
- [ ] **Small Window Size** - UI remains usable at small sizes

---

## 📝 Testing Notes

### Testing Environment
- **OS**: _________________
- **Browser Version** (if web build): _________________
- **App Version**: _________________
- **Server Version**: _________________
- **Date Tested**: _________________

### Known Issues
_(Record any bugs or issues found during testing)_

1. 
2. 
3. 

### Suggestions for Improvement
_(Record any UX improvements or feature requests)_

1. 
2. 
3. 

---

## ✅ Test Summary

**Total Features**: ~200+
**Features Tested**: _____ / _____
**Features Passing**: _____ / _____
**Features Failing**: _____ / _____

**Overall Status**: ⬜ Pass | ⬜ Fail | ⬜ Partial

---

**Last Updated**: 2025-10-30
**Tested By**: _________________

