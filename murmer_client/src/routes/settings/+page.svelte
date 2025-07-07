<script lang="ts">
import { onMount } from 'svelte';
import { get } from 'svelte/store';
import { settings } from '$lib/stores/settings';

let devices: MediaDeviceInfo[] = [];
let selected = '';
let volume = 1;

  onMount(async () => {
    const initial = get(settings);
    selected = initial.inputDeviceId;
    volume = initial.volume;
    try {
      const list = await navigator.mediaDevices.enumerateDevices();
      devices = list.filter((d) => d.kind === 'audioinput');
    } catch (e) {
      console.error('enumerateDevices failed', e);
    }
  });

  $: settings.update((s) => {
    const changed =
      s.inputDeviceId !== selected || s.volume !== volume;
    return changed ? { ...s, inputDeviceId: selected, volume } : s;
  });
</script>

<div class="p-4 space-y-4">
  <h1 class="text-xl font-bold">Settings</h1>

  <div>
    <label class="block mb-1" for="volume">Output Volume</label>
    <input id="volume" type="range" min="0" max="1" step="0.01" bind:value={volume} class="w-full" />
  </div>

  <div>
    <label class="block mb-1" for="mic">Input Device</label>
    <select id="mic" bind:value={selected} class="border p-2 rounded w-full">
      <option value="">Default</option>
      {#each devices as d}
        <option value={d.deviceId}>{d.label || d.deviceId}</option>
      {/each}
    </select>
  </div>
</div>
