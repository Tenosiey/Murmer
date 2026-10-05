import { browser } from '$app/environment';
import { isTauri } from '$lib/platform';
import { hideSpoilers } from '$lib/spoilers';

export async function notify(title: string, rawBody?: string) {
  if (!browser) return;
  // Shown outside the app, where nothing can blur a spoiler.
  const body = rawBody === undefined ? undefined : hideSpoilers(rawBody);

  // Prefer Tauri's notification API when available. The import is dynamic so
  // the plugin never reaches the web bundle, where it could not work anyway.
  if (isTauri) {
    const { isPermissionGranted, requestPermission, sendNotification } = await import(
      '@tauri-apps/plugin-notification'
    );
    let granted = await isPermissionGranted();
    if (!granted) {
      const permission = await requestPermission();
      granted = permission === 'granted';
    }
    if (granted) {
      sendNotification({ title, body });
    }
    return;
  }

  // Fallback to Web Notifications when running in a browser
  if (typeof Notification === 'undefined') return;
  if (Notification.permission === 'granted') {
    new Notification(title, { body });
  } else if (Notification.permission !== 'denied') {
    const permission = await Notification.requestPermission();
    if (permission === 'granted') {
      new Notification(title, { body });
    }
  }
}
