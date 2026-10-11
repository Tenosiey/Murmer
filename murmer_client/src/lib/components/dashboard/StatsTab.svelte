<script lang="ts">
  import { stats, statsConfig } from '$lib/stores/stats';
  import { t } from '$lib/i18n';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  function toggleServerStats(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    // Role-checked server-side; the confirmation arrives as a broadcast
    // stats-config frame which updates the store (and this checkbox).
    stats.setServerEnabled(input.checked);
  }

</script>

{#if active}
  <div class="settings-section">
    <h3 class="section-title">{t('statsTab.lifetimeStats')}</h3>
    <div class="setting-group">
      <label class="toggle-row">
        <input
          type="checkbox"
          checked={$statsConfig?.serverEnabled ?? false}
          disabled={$statsConfig === null}
          onchange={toggleServerStats}
        />
        <span class="toggle-text">
          <span class="toggle-label">{t('statsTab.allowStatTrackingOn')}</span>
          <span class="toggle-description">
            {t('statsTab.letsMembersRecordLifetime')}
          </span>
        </span>
      </label>
      {#if $statsConfig === null}
        <div class="setting-description">{t('statsTab.waitingForTheServer')}</div>
      {/if}
    </div>
    <div class="setting-group">
      <div class="setting-description">
        {t('statsTab.turningThisOffStops')}
      </div>
    </div>
  </div>
{/if}
