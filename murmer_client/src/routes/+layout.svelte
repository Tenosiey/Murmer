<script lang="ts">
  import '../app.css';
  import { WebviewWindow } from '@tauri-apps/api/webviewWindow';

  async function openSettings() {
    const existing = await WebviewWindow.getByLabel('settings');
    if (existing) {
      await existing.show();
      await existing.setFocus();
    } else {
      new WebviewWindow('settings', {
        url: '/settings',
        width: 400,
        height: 300,
        title: 'Settings'
      });
    }
  }
</script>

<div class="fixed top-2 right-2">
  <button class="bg-gray-200 px-2 py-1 rounded" on:click={openSettings} title="Settings">
    ⚙️
  </button>
</div>

<slot />
