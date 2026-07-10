<!--
  Dropdown panel with detailed connection stats: own server ping plus voice
  RTT/jitter/packet loss per peer. Owners/Admins additionally get a list of
  every user's self-reported stats, refreshed while the panel is open.
-->
<script lang="ts">
  import { onDestroy } from 'svelte';
  import { ping } from '$lib/stores/ping';
  import { voiceStats } from '$lib/stores/voice';
  import {
    allConnectionStats,
    canViewAllConnectionStats
  } from '$lib/stores/connectionStats';

  /** How often the admin list refreshes while the panel is open. */
  const REFRESH_INTERVAL_MS = 5000;

  let refreshInterval: number | null = null;

  // Poll the server for everyone's stats while an authorised user has the
  // panel open; the interval dies with the component.
  $: if ($canViewAllConnectionStats && refreshInterval === null) {
    allConnectionStats.request();
    refreshInterval = window.setInterval(
      () => allConnectionStats.request(),
      REFRESH_INTERVAL_MS
    );
  } else if (!$canViewAllConnectionStats && refreshInterval !== null) {
    clearInterval(refreshInterval);
    refreshInterval = null;
  }

  onDestroy(() => {
    if (refreshInterval !== null) clearInterval(refreshInterval);
  });

  function fmtMs(value: number | null): string {
    return value === null ? '—' : `${Math.round(value)} ms`;
  }

  function fmtPct(value: number | null): string {
    return value === null ? '—' : `${(Math.round(value * 10) / 10).toFixed(1)}%`;
  }

  function fmtAge(seconds: number): string {
    return seconds < 15 ? 'just now' : `${Math.round(seconds)}s ago`;
  }

  $: voicePeers = Object.entries($voiceStats);
  $: userList = Object.entries($allConnectionStats).sort(([a], [b]) => a.localeCompare(b));
</script>

<div class="panel" role="dialog" aria-label="Connection stats">
  <div class="section">
    <div class="section-title">Your connection</div>
    <div class="row">
      <span class="label">Server ping</span>
      <span class="value">{$ping > 0 ? `${$ping} ms` : '—'}</span>
    </div>
    {#if voicePeers.length > 0}
      <div class="subsection-title">Voice (per peer)</div>
      {#each voicePeers as [peer, stats] (peer)}
        <div class="row">
          <span class="label peer">{peer}</span>
          <span class="value">
            {fmtMs(stats.rtt)} · {fmtMs(stats.jitter)} jitter · {fmtPct(stats.packetLoss)} loss
          </span>
        </div>
      {/each}
    {:else}
      <div class="row muted-row">
        <span class="label">Voice</span>
        <span class="value">not in a voice channel</span>
      </div>
    {/if}
    <p class="privacy-note">
      Only these quality numbers (ping, voice RTT, jitter, packet loss) are shared with the
      server so admins can help troubleshoot. No IP addresses or device details.
    </p>
  </div>

  {#if $canViewAllConnectionStats}
    <div class="section">
      <div class="section-title">All users</div>
      {#if userList.length === 0}
        <div class="row muted-row">
          <span class="value">No reports yet</span>
        </div>
      {:else}
        <div class="table-scroll">
          <table>
            <thead>
              <tr>
                <th>User</th>
                <th>Ping</th>
                <th>Voice RTT</th>
                <th>Jitter</th>
                <th>Loss</th>
                <th>Updated</th>
              </tr>
            </thead>
            <tbody>
              {#each userList as [user, stats] (user)}
                <tr>
                  <td class="user-cell">{user}</td>
                  <td>{fmtMs(stats.ping)}</td>
                  <td>{fmtMs(stats.voiceRtt)}</td>
                  <td>{fmtMs(stats.voiceJitter)}</td>
                  <td class:loss-warn={stats.voiceLoss !== null && stats.voiceLoss >= 2}
                    >{fmtPct(stats.voiceLoss)}</td>
                  <td class="age-cell">{fmtAge(stats.ageSeconds)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .panel {
    min-width: 20rem;
    max-width: 28rem;
    padding: var(--space-3);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    font-size: var(--text-sm);
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .section + .section {
    border-top: 1px solid var(--color-surface-outline);
    padding-top: var(--space-3);
  }

  .section-title {
    font-weight: 600;
    font-size: var(--text-md);
    margin-bottom: var(--space-1);
  }

  .subsection-title {
    margin-top: var(--space-2);
    font-weight: 500;
    color: var(--color-muted);
    font-size: var(--text-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: var(--space-3);
  }

  .label {
    color: var(--color-on-surface-variant);
  }

  .label.peer {
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .value {
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .muted-row .value {
    color: var(--color-muted);
    font-style: italic;
  }

  .privacy-note {
    margin: var(--space-2) 0 0;
    font-size: var(--text-xs);
    color: var(--color-muted);
    line-height: 1.4;
  }

  .table-scroll {
    overflow-x: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-variant-numeric: tabular-nums;
  }

  th,
  td {
    text-align: right;
    padding: var(--space-1) var(--space-2);
    white-space: nowrap;
  }

  th:first-child,
  td:first-child {
    text-align: left;
    padding-left: 0;
  }

  th {
    font-weight: 500;
    font-size: var(--text-xs);
    color: var(--color-muted);
    border-bottom: 1px solid var(--color-surface-outline);
  }

  .user-cell {
    max-width: 9rem;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .loss-warn {
    color: var(--color-error);
    font-weight: 600;
  }

  .age-cell {
    color: var(--color-muted);
    font-size: var(--text-xs);
  }
</style>
