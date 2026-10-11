<script lang="ts">
  import { dialogs } from '$lib/stores/dialogs';
  import { stats, statsConfig, statsSnapshot } from '$lib/stores/stats';
  import { session } from '$lib/stores/session';
  import UserStatsPanel from '$lib/components/UserStatsPanel.svelte';
  import { t } from '$lib/i18n';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  function toggleStatsOptIn(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    stats.setOptIn(input.checked);
  }

  async function deleteMyStats() {
    const confirmed = await dialogs.confirm({
      title: t('stats.deleteTitle'),
      message: t('stats.deleteMessage'),
      confirmLabel: t('stats.deleteConfirm'),
      danger: true
    });
    if (confirmed) stats.resetStats();
  }

  let statsTracking = $derived(($statsConfig?.serverEnabled ?? false) && ($statsConfig?.optedIn ?? false));
  // Refresh the own snapshot whenever the tab is opened while tracking is on.
  $effect(() => {
    if (active && statsTracking) {
      stats.fetchStats();
    }
  });
  let ownSnapshot =
    $derived($statsSnapshot && $statsSnapshot.user === $session.user ? $statsSnapshot : null);
</script>

{#if active}
  <div class="settings-section">
    <h3 class="section-title">{t('settings.tab.stats')}</h3>

    <div class="setting-group">
      <label class="toggle-row">
        <input
          type="checkbox"
          checked={$statsConfig?.optedIn ?? false}
          disabled={$statsConfig === null || !$statsConfig.serverEnabled}
          onchange={toggleStatsOptIn}
        />
        <span class="toggle-text">
          <span class="toggle-label">{t('stats.track')}</span>
          <span class="toggle-description">
            {t('stats.trackHint')}
          </span>
        </span>
      </label>
      {#if $statsConfig === null}
        <div class="setting-description">{t('stats.waiting')}</div>
      {:else if !$statsConfig.serverEnabled}
        <div class="setting-description">
          {t('stats.serverOff')}
        </div>
      {/if}
      <div>
        <button class="btn btn-danger" onclick={deleteMyStats}>{t('stats.deleteMine')}</button>
      </div>
    </div>

    {#if statsTracking}
      {#if ownSnapshot}
        <UserStatsPanel snapshot={ownSnapshot} />
      {:else}
        <div class="setting-description">{t('stats.loading')}</div>
      {/if}
    {/if}
  </div>
{/if}
