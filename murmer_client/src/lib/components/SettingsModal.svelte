<script lang="ts">
  import { volume } from '$lib/stores/settings';
  export let open: boolean;
  export let close: () => void;
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
      <button on:click={close}>Close</button>
    </div>
  </div>
{/if}
