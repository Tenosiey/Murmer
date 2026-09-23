<script lang="ts">
  import { dialogs } from '$lib/stores/dialogs';
  import { stats, statsConfig, statsSnapshot } from '$lib/stores/stats';
  import { session } from '$lib/stores/session';
  import UserStatsPanel from '$lib/components/UserStatsPanel.svelte';

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
      title: 'Delete your stats?',
      message:
        'This permanently removes all recorded lifetime stats and achievements for your user on this server.',
      confirmLabel: 'Delete stats',
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
    <h3 class="section-title">Stats &amp; Privacy</h3>

    <div class="setting-group">
      <label class="toggle-row">
        <input
          type="checkbox"
          checked={$statsConfig?.optedIn ?? false}
          disabled={$statsConfig === null || !$statsConfig.serverEnabled}
          onchange={toggleStatsOptIn}
        />
        <span class="toggle-text">
          <span class="toggle-label">Track my lifetime stats</span>
          <span class="toggle-description">
            Record activity counters (messages, voice time, reactions, …) and unlock
            achievements. Nothing is recorded unless both this server and you enable it —
            only totals are stored, never message contents. Other members can see your
            stats while you are opted in.
          </span>
        </span>
      </label>
      {#if $statsConfig === null}
        <div class="setting-description">Waiting for the server…</div>
      {:else if !$statsConfig.serverEnabled}
        <div class="setting-description">
          Stat tracking is switched off server-wide. A server Owner or Admin can enable it
          in the server dashboard.
        </div>
      {/if}
      <div>
        <button class="btn btn-danger" onclick={deleteMyStats}>Delete my stats…</button>
      </div>
    </div>

    {#if statsTracking}
      {#if ownSnapshot}
        <UserStatsPanel snapshot={ownSnapshot} />
      {:else}
        <div class="setting-description">Loading your stats…</div>
      {/if}
    {/if}
  </div>
{/if}
