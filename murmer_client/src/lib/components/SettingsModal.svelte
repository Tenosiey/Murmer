<script lang="ts">
  export let open: boolean;
  export let close: () => void;
</script>

{#if open}
  <div class="fixed inset-0 flex items-center justify-center bg-black bg-opacity-50 z-50">
    <div class="bg-white p-4 rounded shadow w-80">
      <h2 class="text-lg font-bold mb-2">Settings</h2>
      <p>Settings go here...</p>
      <button class="mt-4 px-4 py-2 bg-blue-500 text-white rounded" on:click={close}>
        Close
      </button>
    </div>
  </div>
{/if}
