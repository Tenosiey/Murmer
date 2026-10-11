/**
 * Desktop self-update through the Tauri updater plugin, shared by the
 * startup prompt, the "Check for Updates" button and the version-mismatch
 * warning. The plugin is imported dynamically so it stays out of the web
 * bundle, which has no shell to install anything into.
 */
import type { Update } from '@tauri-apps/plugin-updater';
import { APP_VERSION } from './version';
import { dialogs } from './stores/dialogs';
import { isTauri } from './platform';
import { t } from './i18n';

/** Ask the release feed for a newer build; null when this one is current. */
export async function findUpdate(): Promise<Update | null> {
  const { check } = await import('@tauri-apps/plugin-updater');
  return check();
}

/**
 * Ask whether to install `update`; if yes, download, install and relaunch.
 * Returns false when the user declined. `onProgress` receives a line of
 * status text for a caller that has somewhere to show it.
 */
export async function offerUpdate(
  update: Update,
  onProgress: (message: string) => void = () => {}
): Promise<boolean> {
  const install = await dialogs.confirm({
    title: t('about.installTitle'),
    message: t('about.installMessage', { version: update.version, current: APP_VERSION }),
    confirmLabel: t('about.install')
  });
  if (!install) return false;
  const { relaunch } = await import('@tauri-apps/plugin-process');
  let contentLength = 0;
  let downloaded = 0;
  await update.downloadAndInstall((event) => {
    if (event.event === 'Started') {
      contentLength = event.data.contentLength ?? 0;
      onProgress(t('about.downloading'));
    } else if (event.event === 'Progress') {
      downloaded += event.data.chunkLength;
      if (contentLength > 0) {
        onProgress(
          t('about.downloadingPercent', { percent: Math.round((downloaded / contentLength) * 100) })
        );
      }
    } else if (event.event === 'Finished') {
      onProgress(t('about.installing'));
    }
  });
  // On Windows the installer exits the app itself; relaunch covers other platforms.
  await relaunch();
  return true;
}

/**
 * Check once and offer whatever is found. Without this, updates only
 * happened when someone opened Settings → About, so members drifted behind
 * the server. Being offline or up to date stays silent; a failed install
 * after the user said yes does not.
 */
export async function promptForUpdate(): Promise<void> {
  if (!isTauri) return;
  let update: Update | null;
  try {
    update = await findUpdate();
  } catch (e) {
    console.warn('Update check failed', e);
    return;
  }
  if (!update) return;
  try {
    await offerUpdate(update);
  } catch (e) {
    console.error('Update failed', e);
    await dialogs.alert({ title: t('about.updates'), message: t('about.updateFailed') });
  }
}
