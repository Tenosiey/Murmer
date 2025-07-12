<script lang="ts">
  import { volume } from '$lib/stores/settings';
  import { VERSION } from '$lib/version';
  export let open: boolean;
  export let close: () => void;

  let updateMessage = '';

  async function checkUpdates() {
    updateMessage = 'Checking...';
    try {
      const res = await fetch(
        'https://api.github.com/repos/Tenosiey/Murmer/releases/latest'
      );
      if (!res.ok) throw new Error('request failed');
      const data = await res.json();
      const latest: string = data.tag_name || data.name;
      if (latest && latest !== VERSION) {
        updateMessage = `Update available: ${latest}`;
      } else {
        updateMessage = 'You are running the latest version.';
      }
    } catch (e) {
      updateMessage = 'Failed to check for updates.';
    }
  }
</script>

{#if open}
  <div>
    <div>
      <h2>Settings</h2>
      <div>
        <label for="volume-slider">Volume: {Math.round($volume * 100)}</label>
        <input
          id="volume-slider"
          type="range"
          min="0"
          max="1"
          step="0.01"
          bind:value={$volume}
        />
      </div>
      <div>
        <button on:click={checkUpdates}>Check for Updates</button>
        {#if updateMessage}
          <p>{updateMessage}</p>
        {/if}
      </div>
      <button on:click={close}>Close</button>
    </div>
  </div>
{/if}
