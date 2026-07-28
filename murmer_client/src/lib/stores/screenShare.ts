/**
 * Screen share state management.
 *
 * Manages the global screen share instance and tracks active screen shares
 * across all voice channels.
 */
import { writable, derived, get } from 'svelte/store';
import { browser } from '$app/environment';
import { ScreenShareManager } from '../screenshare/manager';
import type {
  ScreenSharePeer,
  ScreenShareActive,
  ScreenShareAudio,
  ScreenShareSettings
} from '../types';
import { chat } from './chat';
import { connection } from './connection';
import { screenShareMuted, screenShareVolume } from './settings';
import type { Message } from '../types';

/**
 * Global screen share manager instance
 */
const screenShareManager = new ScreenShareManager();

/**
 * Store for remote screen share streams
 */
const screenSharePeersStore = writable<ScreenSharePeer[]>([]);

/**
 * Account names whose share we are watching, in the order they were opened —
 * one tile on the viewer stage per entry. Several shares of the same channel
 * can be watched at once; each has its own peer connection, so closing one
 * leaves the others running.
 */
export const watchedScreenShares = writable<string[]>([]);

/**
 * Sharers whose stream has arrived at least once. A watched entry without a
 * peer is still negotiating and has to stay on the stage; one that had a peer
 * and lost it (the sharer stopped, or the connection failed) is gone for good,
 * and leaving its tile up would strand it on "Connecting…" forever.
 */
const streamed = new Set<string>();

/**
 * Per-share audio overrides, keyed by the sharer's account name. Shares
 * without an entry follow the app-wide `screenShareVolume`/`screenShareMuted`
 * settings; watching two shares at once is what makes the distinction matter,
 * since muting one of them must not silence the other.
 */
const screenShareAudioOverrides = writable<Record<string, ScreenShareAudio>>({});

/**
 * The volume and mute state to play a share at: `$shareAudio(user)`, the
 * override if there is one and the app-wide setting otherwise.
 */
export const shareAudio = derived(
  [screenShareAudioOverrides, screenShareVolume, screenShareMuted],
  ([$overrides, $volume, $muted]) =>
    (userId: string): ScreenShareAudio =>
      $overrides[userId] ?? { volume: $volume, muted: $muted }
);

/** Override volume/mute for a single share. */
export function setShareAudio(userId: string, audio: ScreenShareAudio): void {
  screenShareAudioOverrides.update((all) => ({ ...all, [userId]: audio }));
}

screenShareManager.subscribe((peers) => {
  screenSharePeersStore.set(peers);

  // Reconcile the watched list with the manager's peers: a share that was
  // streaming and no longer is has ended, whether or not a `screenshare-stop`
  // frame explained why.
  const live = new Set(peers.map((peer) => peer.userId));
  for (const userId of live) streamed.add(userId);
  for (const userId of get(watchedScreenShares)) {
    if (!live.has(userId) && streamed.has(userId)) closeScreenShare(userId);
  }
});

/**
 * Store tracking which users are actively sharing their screen
 * Key: channelId (number), Value: array of usernames
 */
export const activeScreenShares = writable<Record<number, string[]>>({});

/**
 * Store for local screen share state
 */
export const isScreenSharing = writable<boolean>(false);

/**
 * The sharer's own capture stream while sharing (null otherwise), so the
 * sharer can watch a self-preview and verify what everyone else receives.
 */
export const localScreenShareStream = writable<MediaStream | null>(null);

const PREVIEW_KEY = 'murmer_screenshare_preview';

/**
 * Whether the sharer's self-preview is shown. Decoding and compositing the
 * captured frames a second time costs real CPU/GPU, so the preview can be
 * switched off — when this is false nothing renders the stream at all. The
 * choice is remembered across shares and restarts.
 */
export const screenSharePreview = writable<boolean>(
  browser ? localStorage.getItem(PREVIEW_KEY) !== 'false' : true
);

screenSharePreview.subscribe((value) => {
  if (browser) localStorage.setItem(PREVIEW_KEY, String(value));
});

/** Show/hide the sharer's own preview. */
export function toggleScreenSharePreview(): void {
  screenSharePreview.update((shown) => !shown);
}

// The capture can also end from the browser/OS "stop sharing" bar, which never
// goes through stopScreenShare() — mirror the manager's stream instead of
// flipping the flags at the call sites so that path stays consistent too.
screenShareManager.subscribeLocalStream((stream) => {
  localScreenShareStream.set(stream);
  isScreenSharing.set(stream !== null);
});

/**
 * Store for screen share settings (quality/FPS)
 */
export const screenShareSettings = writable<ScreenShareSettings>({
  width: 1280,
  height: 720,
  frameRate: 30,
  maxBitrate: 5_000_000
});

/**
 * Server-wide bitrate cap in bits per second set by Owners/Admins
 * (null = no cap). Announced after auth and broadcast on change.
 */
export const screenShareServerMaxBitrate = writable<number | null>(null);

chat.on('screenshare-config', (msg: Message) => {
  const raw = (msg as any).maxBitrate;
  const limit =
    typeof raw === 'number' && Number.isFinite(raw) && raw > 0 ? Math.round(raw) : null;
  screenShareServerMaxBitrate.set(limit);
  screenShareManager.setServerMaxBitrate(limit);
});

connection.subscribe((state) => {
  if (state !== 'connected') {
    // Forget the previous server's cap.
    screenShareServerMaxBitrate.set(null);
    screenShareManager.setServerMaxBitrate(null);
    // Without a connection the signaling is gone: stop a running local
    // capture (otherwise it would keep recording invisibly), close every
    // share we were watching and drop the remote share list — channel ids are
    // only unique per server, so stale entries would show phantom "live"
    // badges after a reconnect.
    screenShareManager.stopSharing();
    leaveScreenShareAsViewer();
    activeScreenShares.set({});
  }
});

/**
 * Set the server-wide bitrate cap (Owner/Admin only; enforced server-side).
 * Pass null to remove the cap. Confirmation arrives as a broadcast
 * screenshare-config frame which updates the store.
 */
export function setServerScreenShareMaxBitrate(maxBitrate: number | null): void {
  chat.sendRaw({ type: 'set-screenshare-max-bitrate', maxBitrate });
}

chat.on('screenshare-start', (msg: Message) => {
  const user = msg.user as string;
  const channelId = (msg as any).channelId as number;

  if (user && typeof channelId === 'number') {
    activeScreenShares.update((shares) => {
      const channelShares = shares[channelId] || [];
      if (!channelShares.includes(user)) {
        return {
          ...shares,
          [channelId]: [...channelShares, user]
        };
      }
      return shares;
    });
  }
});

chat.on('screenshare-active', (msg: Message) => {
  const channelId = (msg as any).channelId as number;
  const users = (msg as any).users as string[];

  if (typeof channelId === 'number' && Array.isArray(users)) {
    activeScreenShares.update((shares) => {
      const existing = shares[channelId] || [];
      const merged = [...new Set([...existing, ...users])];
      return { ...shares, [channelId]: merged };
    });
  }
});

chat.on('screenshare-stop', (msg: Message) => {
  const user = msg.user as string;
  const channelId = (msg as any).channelId as number;

  if (user && typeof channelId === 'number') {
    activeScreenShares.update((shares) => {
      const channelShares = shares[channelId] || [];
      return {
        ...shares,
        [channelId]: channelShares.filter((u) => u !== user)
      };
    });
    // Take the tile down explicitly rather than waiting for the peer list to
    // change: a share that stops while we are still negotiating never had a
    // stream, so the reconciliation above would leave its tile connecting for
    // good.
    closeScreenShare(user);
  }
});

/**
 * Start sharing screen
 */
export async function startScreenShare(user: string, channelId: number): Promise<void> {
  const settings = get(screenShareSettings);
  await screenShareManager.startSharing(user, channelId, settings);
}

/**
 * Stop sharing screen
 */
export function stopScreenShare(): void {
  screenShareManager.stopSharing();
}

/**
 * Start watching a user's screen share. The tile appears immediately and
 * shows a placeholder until the stream arrives — connecting takes a moment,
 * and a click that seems to do nothing invites a second one.
 */
export async function openScreenShare(
  userId: string,
  viewerName?: string,
  channelId?: number
): Promise<void> {
  watchedScreenShares.update((watched) =>
    watched.includes(userId) ? watched : [...watched, userId]
  );
  await screenShareManager.viewScreenShare(userId, viewerName, channelId);
}

/**
 * Stop watching one user's screen share, leaving every other share open. Its
 * peer connection is closed, so the stream stops being sent to this client.
 */
export function closeScreenShare(userId: string): void {
  watchedScreenShares.update((watched) => watched.filter((name) => name !== userId));
  streamed.delete(userId);
  screenShareAudioOverrides.update((audio) => {
    if (!(userId in audio)) return audio;
    const next = { ...audio };
    delete next[userId];
    return next;
  });
  screenShareManager.stopViewing(userId);
}

/** Stop watching every share at once. */
export function closeAllScreenShares(): void {
  for (const userId of get(watchedScreenShares)) closeScreenShare(userId);
}

/**
 * Leave screen share session as a viewer
 */
export function leaveScreenShareAsViewer(): void {
  closeAllScreenShares();
  screenShareManager.leaveAsViewer();
}

/**
 * Update screen share quality settings
 */
export function updateScreenShareSettings(settings: Partial<ScreenShareSettings>): void {
  screenShareSettings.update((current) => ({ ...current, ...settings }));
  screenShareManager.updateSettings(settings);
}

/**
 * Export screen share peers for consumption
 */
export const screenSharePeers = {
  subscribe: screenSharePeersStore.subscribe
};

if (typeof window !== 'undefined') {
  window.addEventListener('beforeunload', () => {
    screenShareManager.destroy();
  });
}
