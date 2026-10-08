<script lang="ts">
  import { APP_VERSION } from '$lib/version';
  import MurmerLogo from '$lib/components/MurmerLogo.svelte';
  import { isTauri } from '$lib/platform';
  import { dialogs } from '$lib/stores/dialogs';
  import { t } from '$lib/i18n';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  let updateMessage = $state('');
  let updateTone: 'success' | 'warning' | null = $state(null);
  let updating = $state(false);

  // Launch on login. Read back from the OS each time the tab opens rather
  // than cached: the user can switch it off in their system settings too.
  // `null` until the answer arrives, which keeps the toggle disabled.
  let autostart: boolean | null = $state(null);

  $effect(() => {
    if (!active || !isTauri) return;
    import('@tauri-apps/plugin-autostart')
      .then(({ isEnabled }) => isEnabled())
      .then((enabled) => (autostart = enabled))
      .catch((e) => console.error('Could not read launch on login', e));
  });

  async function toggleAutostart(event: Event) {
    const wanted = (event.currentTarget as HTMLInputElement).checked;
    autostart = null;
    try {
      const { enable, disable, isEnabled } = await import('@tauri-apps/plugin-autostart');
      await (wanted ? enable() : disable());
      autostart = await isEnabled();
    } catch (e) {
      console.error('Could not change launch on login', e);
      autostart = !wanted;
    }
  }

  const REPO_URL = 'https://github.com/Tenosiey/Murmer';
  const ABOUT_LINKS = [
    { label: t('about.github'), url: REPO_URL },
    { label: t('about.reportIssue'), url: `${REPO_URL}/issues` },
    { label: t('about.releases'), url: `${REPO_URL}/releases` },
    { label: t('about.license'), url: `${REPO_URL}/blob/main/LICENSE` }
  ];

  async function checkUpdates() {
    if (updating) return;
    updating = true;
    updateMessage = t('about.checking');
    updateTone = null;
    try {
      // Dynamic so the updater plugin stays out of the web bundle, which has
      // no shell to install anything into.
      const [{ check }, { relaunch }] = await Promise.all([
        import('@tauri-apps/plugin-updater'),
        import('@tauri-apps/plugin-process')
      ]);
      const update = await check();
      if (!update) {
        updateMessage = t('about.upToDate');
        updateTone = 'success';
        return;
      }
      updateMessage = t('about.updateAvailable', { version: update.version });
      updateTone = 'warning';
      const install = await dialogs.confirm({
        title: t('about.installTitle'),
        message: t('about.installMessage', { version: update.version, current: APP_VERSION }),
        confirmLabel: t('about.install')
      });
      if (!install) return;
      let contentLength = 0;
      let downloaded = 0;
      await update.downloadAndInstall((event) => {
        if (event.event === 'Started') {
          contentLength = event.data.contentLength ?? 0;
          updateMessage = t('about.downloading');
          updateTone = null;
        } else if (event.event === 'Progress') {
          downloaded += event.data.chunkLength;
          if (contentLength > 0) {
            updateMessage = t('about.downloadingPercent', {
              percent: Math.round((downloaded / contentLength) * 100)
            });
          }
        } else if (event.event === 'Finished') {
          updateMessage = t('about.installing');
        }
      });
      // On Windows the installer exits the app itself; relaunch covers other platforms.
      await relaunch();
    } catch (e) {
      console.error('Update failed', e);
      updateMessage = t('about.updateFailed');
      updateTone = null;
    } finally {
      updating = false;
    }
  }
</script>

{#if active}
  <div class="settings-section">
    <h3 class="section-title">{t('settings.tab.about')}</h3>

    <div class="setting-group">
      <div class="about-header">
        <MurmerLogo size={32} />
        <span class="about-name">Murmer</span>
        <span class="setting-value">v{APP_VERSION}</span>
      </div>
      <div class="setting-description">{t('about.description')}</div>
    </div>

    {#if isTauri}
    <div class="setting-group">
      <label class="toggle-row">
        <input
          type="checkbox"
          checked={autostart ?? false}
          disabled={autostart === null}
          onchange={toggleAutostart}
        />
        <span class="toggle-text">
          <span class="toggle-label">{t('about.launchOnLogin')}</span>
          <span class="toggle-description">{t('about.launchOnLoginHint')}</span>
        </span>
      </label>
    </div>

    <div class="setting-group">
      <span class="setting-label">{t('about.updates')}</span>
      <button class="btn update-btn" onclick={checkUpdates} disabled={updating}>{t('about.checkUpdates')}</button>
      {#if updateMessage}
        <div class="update-message" class:success={updateTone === 'success'} class:warning={updateTone === 'warning'}>
          {updateMessage}
        </div>
      {/if}
    </div>
    {:else}
    <div class="setting-group">
      <span class="setting-label">{t('about.updates')}</span>
      <div class="setting-description">{t('about.webClientUpdates')}</div>
    </div>
    {/if}

    <div class="setting-group">
      <span class="setting-label">{t('about.links')}</span>
      <div class="about-links">
        {#each ABOUT_LINKS as link}
          <a class="btn about-link" href={link.url} target="_blank" rel="noopener noreferrer">
            {link.label}
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
              <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"></path>
              <polyline points="15,3 21,3 21,9"></polyline>
              <line x1="10" y1="14" x2="21" y2="3"></line>
            </svg>
          </a>
        {/each}
      </div>
    </div>

    <div class="setting-group">
      <div class="setting-description">{t('about.builtWith')}</div>
    </div>
  </div>
{/if}

<style>

  .update-btn {
    justify-self: start;
  }

  .about-header {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .about-name {
    font-size: var(--text-lg);
    font-weight: 600;
    color: var(--color-on-surface);
  }

  .about-links {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .about-link {
    text-decoration: none;
    gap: var(--space-2);
  }

  .update-message {
    font-size: var(--text-sm);
    color: var(--color-muted);
  }

  .update-message.success {
    color: var(--color-success);
  }

  .update-message.warning {
    color: var(--color-warning);
  }
</style>
