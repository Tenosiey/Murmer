<script lang="ts">
  import { volume } from '$lib/stores/settings';
  export let open: boolean;
  export let close: () => void;
</script>

{#if open}
  <div class="fixed inset-0 flex items-center justify-center bg-black bg-opacity-50 z-50">
    <div class="bg-white p-4 rounded shadow w-80">
      <h2 class="text-lg font-bold mb-2">Settings</h2>
      <div class="mb-4">
        <label for="volume-slider" class="block text-sm mb-1"
          >Volume: {Math.round($volume * 100)}</label
        >
        <input
          id="volume-slider"
          type="range"
          min="0"
          max="1"
          step="0.01"
          bind:value={$volume}
          class="w-full"
        />
      </div>
      <button class="mt-4 px-4 py-2 bg-blue-500 text-white rounded" on:click={close}>
        Close
      </button>
    </div>
  </div>
{/if}
