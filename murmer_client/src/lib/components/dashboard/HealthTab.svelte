<script lang="ts">
  import {
    serverMetrics,
    formatUptime,
    formatMs,
    METRICS_POLL_INTERVAL_MS
  } from '$lib/stores/serverMetrics';

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
    <h3 class="section-title">Health</h3>
    <div class="setting-group">
      <div class="setting-description">
        What this server is doing right now. The counters are kept in memory and
        start again from zero at every restart, so they describe the current run
        and nothing before it. Rates are measured across refreshes, which happen
        every {METRICS_POLL_INTERVAL_MS / 1000} seconds while this tab is open.
      </div>
      {#if $serverMetrics === null}
        <div class="setting-description">Waiting for the server…</div>
      {:else}
        <ul class="metric-grid">
          <li class="metric">
            <span class="metric-value">{formatCount($serverMetrics.connections)}</span>
            <span class="metric-label">Connections open</span>
            <span class="metric-note">
              peak {formatCount($serverMetrics.peakConnections)} this run
            </span>
          </li>
          <li class="metric">
            <span class="metric-value">
              {$serverMetrics.framesPerSecond.toFixed(1)}<span class="metric-unit">/s</span>
            </span>
            <span class="metric-label">Frames received</span>
            <span class="metric-note">
              {formatCount($serverMetrics.frames)} total
            </span>
          </li>
          <li class="metric">
            <span class="metric-value">
              {$serverMetrics.dbAverageMs === null
                ? '—'
                : formatMs($serverMetrics.dbAverageMs)}
            </span>
            <span class="metric-label">Database call</span>
            <span class="metric-note">
              {$serverMetrics.dbAverageMs === null ? 'idle · ' : ''}worst
              {formatMs($serverMetrics.dbMaxMs)} ·
              {formatCount($serverMetrics.dbCalls)} calls
            </span>
          </li>
          <li class="metric">
            <span class="metric-value">{formatUptime($serverMetrics.uptimeSeconds)}</span>
            <span class="metric-label">Uptime</span>
            <span class="metric-note">since the last restart</span>
          </li>
        </ul>
      {/if}
    </div>

    <div class="setting-group">
      <span class="setting-label">Rate-limit rejections</span>
      <div class="setting-description">
        Requests turned away since the server started. A climbing auth count is
        somebody guessing keys; climbing messages or uploads is either a member
        flooding or a limit set too low for the room. All four limits are set by
        the server's environment, not from here.
      </div>
      {#if $serverMetrics === null}
        <div class="setting-description">Waiting for the server…</div>
      {:else}
        <ul class="storage-list">
          {#each [
            { label: 'Messages', value: $serverMetrics.rejectedMessages },
            { label: 'Authentication', value: $serverMetrics.rejectedAuth },
            { label: 'Uploads', value: $serverMetrics.rejectedUploads },
            { label: 'Replayed signatures', value: $serverMetrics.rejectedReplays }
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
