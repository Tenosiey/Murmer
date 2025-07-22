<!--
  Settings modal allowing users to adjust audio devices and check for updates.
  The component renders nothing when the `open` prop is false.
-->
<script lang="ts">
  import { volume, inputDeviceId, outputDeviceId } from '$lib/stores/settings';
  import { APP_VERSION } from '$lib/version';
  import { onMount } from 'svelte';
  export let open: boolean;
  export let close: () => void;

  let updateMessage = '';

  let inputs: MediaDeviceInfo[] = [];
  let outputs: MediaDeviceInfo[] = [];

  onMount(async () => {
    try {
      const devices = await navigator.mediaDevices.enumerateDevices();
      inputs = devices.filter((d) => d.kind === 'audioinput');
      outputs = devices.filter((d) => d.kind === 'audiooutput');
    } catch (e) {
      console.error('Failed to enumerate devices', e);
    }
  });

  async function checkUpdates() {
    updateMessage = 'Checking...';
    try {
      const res = await fetch(
        'https://api.github.com/repos/Tenosiey/Murmer/releases?per_page=10'
      );
      if (!res.ok) throw new Error('request failed');
      const releases = await res.json();
      if (Array.isArray(releases) && releases.length > 0) {
        const stable = releases.find((r) => !r.prerelease);
        const pre = releases.find((r) => r.prerelease);
        if (pre && pre.tag_name && pre.tag_name !== APP_VERSION) {
          updateMessage = `Pre-release available: ${pre.tag_name}`;
        } else if (stable && stable.tag_name && stable.tag_name !== APP_VERSION) {
          updateMessage = `Update available: ${stable.tag_name}`;
        } else {
          updateMessage = 'You are running the latest version.';
        }
      } else {
        updateMessage = 'No releases found.';
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
        <label for="input-select">Input Device:</label>
        <select id="input-select" bind:value={$inputDeviceId}>
          <option value="">Default</option>
          {#each inputs as dev}
            <option value={dev.deviceId}>{dev.label || dev.deviceId}</option>
          {/each}
        </select>
      </div>
      <div>
        <label for="output-select">Output Device:</label>
        <select id="output-select" bind:value={$outputDeviceId}>
          <option value="">Default</option>
          {#each outputs as dev}
            <option value={dev.deviceId}>{dev.label || dev.deviceId}</option>
          {/each}
        </select>
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
