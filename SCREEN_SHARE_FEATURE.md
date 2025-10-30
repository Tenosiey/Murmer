# Screen Share Feature

## Overview

A complete screen sharing feature has been implemented for Murmer, allowing users in voice channels to share their screens with customizable quality and frame rate settings. The implementation follows Discord's user experience while providing more granular control over streaming quality.

## Features Implemented

### 1. **Screen Share Manager** (`murmer_client/src/lib/screenshare/manager.ts`)
- WebRTC-based peer-to-peer screen sharing
- Configurable quality settings (resolution and frame rate)
- Automatic cleanup when user stops sharing via browser UI
- ICE negotiation and connection management
- Support for viewing multiple screen shares

### 2. **Quality Presets**
Users can choose from predefined quality presets:
- **480p** - 854x480 @ 15fps (low bandwidth)
- **720p** - 1280x720 @ 30fps (balanced, default)
- **1080p** - 1920x1080 @ 30fps (high quality)
- **1080p60** - 1920x1080 @ 60fps (smooth high quality)
- **1440p** - 2560x1440 @ 30fps (very high quality)
- **4K** - 3840x2160 @ 30fps (maximum quality)

Or configure custom settings with:
- Custom resolution (width/height)
- Custom frame rate (15-60 fps)

### 3. **UI Components**

#### **ScreenShareControls** (`murmer_client/src/lib/components/ScreenShareControls.svelte`)
- Start/stop screen sharing button
- Quality preset selector
- Custom quality settings panel
- Only enabled when user is in a voice channel
- Settings can only be changed when not actively sharing

#### **ScreenShareViewer** (`murmer_client/src/lib/components/ScreenShareViewer.svelte`)
- Modal window displaying remote screen share
- Fullscreen toggle (F key or button)
- Close on Escape key or X button
- Click outside to close
- Shows username of sharing user

#### **Screen Share Indicator**
- Small monitor icon appears next to users who are sharing their screen
- Displayed in the voice channel user list
- Click the icon to open the viewer and watch the screen share

### 4. **Backend Support** (`murmer_server/src/ws/handlers.rs`)
Added handlers for screen share signaling messages:
- `screenshare-start` - Broadcast when a user starts sharing
- `screenshare-stop` - Broadcast when a user stops sharing
- `screenshare-offer` - WebRTC offer signaling
- `screenshare-answer` - WebRTC answer signaling
- `screenshare-candidate` - ICE candidate exchange

### 5. **State Management** (`murmer_client/src/lib/stores/screenShare.ts`)
- Tracks active screen shares per voice channel
- Manages local screen share state
- Stores screen share settings (persists quality preferences)
- Provides functions to start/stop sharing and viewing

## Usage

### Starting a Screen Share

1. **Join a voice channel** - You must be in a voice channel to share your screen
2. **Configure quality** (optional):
   - Click the settings gear icon next to "Share Screen"
   - Select a quality preset (720p is default)
   - Or enable "Custom Settings" for precise control
3. **Click "Share Screen"**
4. **Select what to share** in the browser dialog:
   - Entire screen
   - Application window
   - Browser tab
5. The screen share will start and other users will see an indicator icon

### Viewing a Screen Share

1. Look for the monitor icon (📺) next to a user's name in the voice channel
2. Click the icon to open the viewer
3. Use the fullscreen button or press F to toggle fullscreen
4. Press Escape or click the X to close the viewer

### Stopping Your Screen Share

1. Click "Stop Sharing" in the screen share controls, or
2. Stop sharing via your browser's native screen share controls

## Technical Details

### WebRTC Architecture

- Uses the same ICE/STUN infrastructure as voice chat
- Peer-to-peer connections for low latency
- Automatic reconnection on network issues
- Graceful degradation on connection failures

### Quality Control

Screen share settings are applied when starting a new share:
- Resolution: Requested as "ideal" constraints (browser may adjust)
- Frame rate: Browser will target the requested FPS
- Higher quality requires more bandwidth and CPU

### Browser Compatibility

Built on standard WebRTC APIs:
- `navigator.mediaDevices.getDisplayMedia()` for screen capture
- Full support in Chrome, Edge, Firefox, and Safari (modern versions)
- Requires HTTPS (or localhost for development)

### Security

- Users must explicitly grant permission via browser dialog
- Permissions are per-session (no persistent access)
- Screen sharing is tied to voice channel membership
- All WebRTC connections use DTLS encryption

## Code Structure

```
murmer_client/src/lib/
├── screenshare/
│   └── manager.ts              # Core WebRTC screen share logic
├── stores/
│   └── screenShare.ts          # State management and stores
├── components/
│   ├── ScreenShareControls.svelte   # UI for starting/configuring shares
│   └── ScreenShareViewer.svelte     # Modal viewer for remote shares
└── types.ts                    # TypeScript interfaces (extended)

murmer_server/src/ws/
└── handlers.rs                 # Backend signaling handlers (extended)
```

## Future Enhancements

Potential improvements for future iterations:

1. **System Audio Sharing** - Include audio from shared applications
2. **Bandwidth Monitoring** - Display current bandwidth usage
3. **Recording** - Allow users to record screen shares
4. **Annotations** - Draw on shared screens in real-time
5. **Thumbnail Preview** - Small preview in the sidebar
6. **Multiple Viewers** - Track who is viewing your share
7. **Bitrate Control** - Manual bitrate limits for bandwidth management
8. **Adaptive Quality** - Automatically adjust quality based on network conditions

## Testing Checklist

- [ ] Screen share starts successfully in a voice channel
- [ ] Quality presets apply correctly
- [ ] Custom settings work as expected
- [ ] Screen share indicator appears for other users
- [ ] Clicking indicator opens viewer with correct stream
- [ ] Fullscreen mode works (F key and button)
- [ ] Viewer closes on Escape, X, and click-outside
- [ ] Screen share stops when browser sharing ends
- [ ] "Stop Sharing" button stops the share
- [ ] Multiple users can share simultaneously (each gets own indicator)
- [ ] Viewer works for viewing different users' shares
- [ ] Settings persist between shares
- [ ] Works across different browsers
- [ ] No memory leaks on repeated start/stop cycles

## Known Limitations

1. **Browser Permissions**: Users must grant screen sharing permission each time (can't be automated for security)
2. **Quality Constraints**: Actual resolution/FPS may differ from requested based on browser/system capabilities
3. **Bandwidth**: High-quality screen shares (1440p, 4K) require significant upload bandwidth
4. **CPU Usage**: Encoding high-resolution streams can be CPU-intensive on older hardware
5. **Audio**: System audio sharing is not currently supported

## Troubleshooting

**Screen share button is disabled**
- You must join a voice channel first

**Permission denied when trying to share**
- Check browser permissions for screen sharing
- Ensure the site is served over HTTPS (or localhost)

**Poor quality or choppy playback**
- Reduce the quality preset
- Check network bandwidth (both sharer and viewer)
- Close other bandwidth-intensive applications

**Viewer shows black screen**
- The sharer may have stopped sharing
- Try closing and reopening the viewer
- Check browser console for WebRTC errors

**Can't see the screen share indicator**
- Ensure the user has successfully started sharing
- Check that you're both in the same voice channel
- Refresh the page if the state is out of sync

---

Built with ❤️ using WebRTC, SvelteKit, and Rust/Axum

