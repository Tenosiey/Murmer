<script lang="ts">
  import { APP_VERSION } from '$lib/version';
  import MurmerLogo from '$lib/components/MurmerLogo.svelte';
  import { isTauri } from '$lib/platform';
  import { dialogs } from '$lib/stores/dialogs';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  let updateMessage = $state('');
  let updating = $state(false);

  const REPO_URL = 'https://github.com/Tenosiey/Murmer';
  const ABOUT_LINKS = [
    { label: 'GitHub repository', url: REPO_URL },
    { label: 'Report an issue', url: `${REPO_URL}/issues` },
    { label: 'Releases & changelog', url: `${REPO_URL}/releases` },
    { label: 'License', url: `${REPO_URL}/blob/main/LICENSE` }
  ];

  async function checkUpdates() {
    if (updating) return;
    updating = true;
    updateMessage = 'Checking...';
    try {
      // Dynamic so the updater plugin stays out of the web bundle, which has
      // no shell to install anything into.
      const [{ check }, { relaunch }] = await Promise.all([
        import('@tauri-apps/plugin-updater'),
        import('@tauri-apps/plugin-process')
      ]);
      const update = await check();
      if (!update) {
        updateMessage = 'You are running the latest version.';
        return;
      }
      updateMessage = `Update available: ${update.version}`;
      const install = await dialogs.confirm({
        title: 'Install update?',
        message: `Version ${update.version} is available (you have ${APP_VERSION}). The app will restart after installing.`,
        confirmLabel: 'Install'
      });
      if (!install) return;
      let contentLength = 0;
      let downloaded = 0;
      await update.downloadAndInstall((event) => {
        if (event.event === 'Started') {
          contentLength = event.data.contentLength ?? 0;
          updateMessage = 'Downloading update...';
        } else if (event.event === 'Progress') {
          downloaded += event.data.chunkLength;
          if (contentLength > 0) {
            updateMessage = `Downloading update... ${Math.round((downloaded / contentLength) * 100)}%`;
          }
        } else if (event.event === 'Finished') {
          updateMessage = 'Installing update...';
        }
      });
      // On Windows the installer exits the app itself; relaunch covers other platforms.
      await relaunch();
    } catch (e) {
      console.error('Update failed', e);
      updateMessage = 'Update failed. Try again or download the latest release from GitHub.';
    } finally {
      updating = false;
    }
  }
</script>

{#if active}
  <div class="settings-section">
    <h3 class="section-title">About</h3>

    <div class="setting-group">
      <div class="about-header">
        <MurmerLogo size={32} />
        <span class="about-name">Murmer</span>
        <span class="setting-value">v{APP_VERSION}</span>
      </div>
      <div class="setting-description">
        A self-hosted voice and text chat prototype. Murmer is open source —
        the client and server live in the same repository on GitHub.
      </div>
    </div>

    {#if isTauri}
    <div class="setting-group">
      <span class="setting-label">Updates</span>
      <button class="btn update-btn" onclick={checkUpdates} disabled={updating}>Check for Updates</button>
      {#if updateMessage}
        <div class="update-message" class:success={updateMessage.startsWith('You are running')} class:warning={updateMessage.startsWith('Update available')}>
          {updateMessage}
        </div>
      {/if}
    </div>
    {:else}
    <div class="setting-group">
      <span class="setting-label">Updates</span>
      <div class="setting-description">
        You are running the web client, which is served by the site you opened — reload the
        page to pick up a new version.
      </div>
    </div>
    {/if}

    <div class="setting-group">
      <span class="setting-label">Links</span>
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
      <div class="setting-description">
        Built with Tauri, SvelteKit and Rust (Axum). Released under the MIT License.
      </div>
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
