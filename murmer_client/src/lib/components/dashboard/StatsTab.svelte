<script lang="ts">
  import { stats, statsConfig } from '$lib/stores/stats';

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
    <h3 class="section-title">Lifetime Stats</h3>
    <div class="setting-group">
      <label class="toggle-row">
        <input
          type="checkbox"
          checked={$statsConfig?.serverEnabled ?? false}
          disabled={$statsConfig === null}
          onchange={toggleServerStats}
        />
        <span class="toggle-text">
          <span class="toggle-label">Allow stat tracking on this server</span>
          <span class="toggle-description">
            Lets members record lifetime stats (messages sent, voice minutes, reactions, …)
            and unlock achievements. Tracking is double opt-in: even with this enabled,
            nothing is recorded for a member until they opt in themselves in their own
            settings. Only aggregate counters are stored — never message contents.
          </span>
        </span>
      </label>
      {#if $statsConfig === null}
        <div class="setting-description">Waiting for the server…</div>
      {/if}
    </div>
    <div class="setting-group">
      <div class="setting-description">
        Turning this off stops all recording immediately. Already recorded stats are kept
        but hidden until tracking is enabled again; each member can delete their own
        recorded stats at any time from Settings → Stats &amp; Privacy.
      </div>
    </div>
  </div>
{/if}
