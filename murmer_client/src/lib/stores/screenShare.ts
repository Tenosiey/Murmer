/**
 * Screen share state management.
 *
 * Manages the global screen share instance and tracks active screen shares
 * across all voice channels.
 */
import { writable, derived, get } from 'svelte/store';
import { ScreenShareManager } from '../screenshare/manager';
import type { ScreenSharePeer, ScreenShareActive, ScreenShareSettings } from '../types';
import { chat } from './chat';
import type { Message } from '../types';

/**
 * Global screen share manager instance
 */
const screenShareManager = new ScreenShareManager();

/**
 * Store for remote screen share streams
 */
const screenSharePeersStore = writable<ScreenSharePeer[]>([]);

// Subscribe to screen share manager updates
screenShareManager.subscribe((peers) => {
  screenSharePeersStore.set(peers);
});

/**
 * Store tracking which users are actively sharing their screen
 * Key: channelName, Value: array of usernames
 */
export const activeScreenShares = writable<Record<string, string[]>>({});

/**
 * Store for local screen share state
 */
export const isScreenSharing = writable<boolean>(false);

/**
 * Store for screen share settings (quality/FPS)
 */
export const screenShareSettings = writable<ScreenShareSettings>({
  width: 1280,
  height: 720,
  frameRate: 30
});

/**
 * Initialize screen share event listeners
 */
chat.on('screenshare-start', (msg: Message) => {
  const user = msg.user as string;
  const channel = msg.channel as string;
  
  if (user && channel) {
    activeScreenShares.update(shares => {
      const channelShares = shares[channel] || [];
      if (!channelShares.includes(user)) {
        return {
          ...shares,
          [channel]: [...channelShares, user]
        };
      }
      return shares;
    });
  }
});

chat.on('screenshare-stop', (msg: Message) => {
  const user = msg.user as string;
  const channel = msg.channel as string;
  
  if (user && channel) {
    activeScreenShares.update(shares => {
      const channelShares = shares[channel] || [];
      return {
        ...shares,
        [channel]: channelShares.filter(u => u !== user)
      };
    });
  }
});

/**
 * Start sharing screen
 */
export async function startScreenShare(user: string, channel: string): Promise<void> {
  const settings = get(screenShareSettings);
  await screenShareManager.startSharing(user, channel, settings);
  isScreenSharing.set(true);
}

/**
 * Stop sharing screen
 */
export function stopScreenShare(): void {
  screenShareManager.stopSharing();
  isScreenSharing.set(false);
}

/**
 * View a user's screen share
 */
export async function viewScreenShare(userId: string, viewerName?: string, channel?: string): Promise<void> {
  await screenShareManager.viewScreenShare(userId, viewerName, channel);
}

/**
 * Stop viewing a user's screen share
 */
export function stopViewingScreenShare(userId: string): void {
  screenShareManager.stopViewing(userId);
}

/**
 * Leave screen share session as a viewer
 */
export function leaveScreenShareAsViewer(): void {
  screenShareManager.leaveAsViewer();
}

/**
 * Update screen share quality settings
 */
export function updateScreenShareSettings(settings: Partial<ScreenShareSettings>): void {
  screenShareSettings.update(current => ({ ...current, ...settings }));
  screenShareManager.updateSettings(settings);
}

/**
 * Export screen share peers for consumption
 */
export const screenSharePeers = {
  subscribe: screenSharePeersStore.subscribe
};

/**
 * Cleanup on module unload
 */
if (typeof window !== 'undefined') {
  window.addEventListener('beforeunload', () => {
    screenShareManager.destroy();
  });
}
