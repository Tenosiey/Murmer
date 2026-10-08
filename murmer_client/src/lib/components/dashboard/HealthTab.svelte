<script lang="ts">
  import {
    serverMetrics,
    formatUptime,
    formatMs,
    METRICS_POLL_INTERVAL_MS
  } from '$lib/stores/serverMetrics';
  import { t } from '$lib/i18n';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  // The only live readout in the dashboard, so the only one that polls. The
  // counters are cumulative; two samples are what turn them into a rate, so
  // the first refresh after opening the tab still shows averages since the
  // server started rather than nothing at all.
  $effect(() => {
    if (!active) return;
    serverMetrics.refresh();
    const timer = setInterval(() => serverMetrics.refresh(), METRICS_POLL_INTERVAL_MS);
    return () => clearInterval(timer);
  });

  /** Thousands separators: these counters reach seven figures on a busy day. */
  function formatCount(value: number): string {
    return value.toLocaleString();
  }

</script>

{#if active}
  <div class="settings-section">
    <h3 class="section-title">{t('dashboard.tab.health')}</h3>
    <div class="setting-group">
      <div class="setting-description">
        {t('health.intro', { seconds: METRICS_POLL_INTERVAL_MS / 1000 })}
      </div>
      {#if $serverMetrics === null}
        <div class="setting-description">{t('health.waiting')}</div>
      {:else}
        <ul class="metric-grid">
          <li class="metric">
            <span class="metric-value">{formatCount($serverMetrics.connections)}</span>
            <span class="metric-label">{t('health.connections')}</span>
            <span class="metric-note">
              {t('health.peak', { count: formatCount($serverMetrics.peakConnections) })}
            </span>
          </li>
          <li class="metric">
            <span class="metric-value">
              {$serverMetrics.framesPerSecond.toFixed(1)}<span class="metric-unit">/s</span>
            </span>
            <span class="metric-label">{t('health.frames')}</span>
            <span class="metric-note">
              {t('health.framesTotal', { count: formatCount($serverMetrics.frames) })}
            </span>
          </li>
          <li class="metric">
            <span class="metric-value">
              {$serverMetrics.dbAverageMs === null
                ? '—'
                : formatMs($serverMetrics.dbAverageMs)}
            </span>
            <span class="metric-label">{t('health.dbCall')}</span>
            <span class="metric-note">
              {$serverMetrics.dbAverageMs === null ? t('health.idle') : ''}{t('health.dbNote', {
                worst: formatMs($serverMetrics.dbMaxMs),
                calls: formatCount($serverMetrics.dbCalls)
              })}
            </span>
          </li>
          <li class="metric">
            <span class="metric-value">{formatUptime($serverMetrics.uptimeSeconds)}</span>
            <span class="metric-label">{t('health.uptime')}</span>
            <span class="metric-note">{t('health.sinceRestart')}</span>
          </li>
        </ul>
      {/if}
    </div>

    <div class="setting-group">
      <span class="setting-label">{t('health.rateLimits')}</span>
      <div class="setting-description">
        {t('health.rateLimitsHint')}
      </div>
      {#if $serverMetrics === null}
        <div class="setting-description">{t('health.waiting')}</div>
      {:else}
        <ul class="storage-list">
          {#each [
            { label: t('health.messages'), value: $serverMetrics.rejectedMessages },
            { label: t('health.auth'), value: $serverMetrics.rejectedAuth },
            { label: t('health.uploads'), value: $serverMetrics.rejectedUploads },
            { label: t('health.requests'), value: $serverMetrics.rejectedFrames },
            { label: t('health.linkPreviews'), value: $serverMetrics.rejectedPreviews }
          ] as row (row.label)}
            <li class="storage-row">
              <span class="storage-label">{row.label}</span>
              <span class="storage-value">{formatCount(row.value)}</span>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>
{/if}

<style>
  .metric-grid {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(9rem, 1fr));
    gap: var(--space-2);
  }

  .metric {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--color-surface-raised);
  }

  .metric-value {
    font-size: var(--text-lg);
    color: var(--color-on-surface);
    /* The numbers change every few seconds in place; a proportional font
       would make the whole tile shuffle sideways on every refresh. */
    font-variant-numeric: tabular-nums;
  }

  .metric-unit {
    font-size: var(--text-sm);
    color: var(--color-muted);
  }

  .metric-label {
    font-size: var(--text-sm);
    color: var(--color-on-surface);
  }

  .metric-note {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }
</style>
