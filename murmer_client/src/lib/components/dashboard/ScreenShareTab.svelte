<script lang="ts">
  import { onMount } from 'svelte';
  import { onServerError } from '$lib/stores/chat';
  import { t } from '$lib/i18n';
  import {
    screenShareServerMaxBitrate,
    setServerScreenShareMaxBitrate
  } from '$lib/stores/screenShare';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  // Filled from the server's answer when the dashboard opens, then left
  // alone: incoming broadcasts must not clobber what the user is typing.
  let screenShareCapMbps = $state(($screenShareServerMaxBitrate ?? 0) / 1_000_000);
  let screenShareFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);
  /** Set after sending a save; cleared once the broadcast confirms the
      change or an error frame arrives. */
  let screenShareSavePending = $state(false);

  let screenShareCapDirty = $derived(
    Math.round((Number.isFinite(screenShareCapMbps) ? screenShareCapMbps : 0) * 1_000_000) !==
      ($screenShareServerMaxBitrate ?? 0)
  );

  // A broadcast matching the submitted value confirms the save.
  $effect(() => {
    if (screenShareSavePending && !screenShareCapDirty) {
      screenShareSavePending = false;
      screenShareFeedback = { text: t('screenshareTab.saved'), kind: 'info' };
    }
  });

  function saveScreenShareCap() {
    const mbps = Number.isFinite(screenShareCapMbps) ? screenShareCapMbps : NaN;
    if (Number.isNaN(mbps) || mbps < 0 || mbps > 100 || (mbps > 0 && mbps < 0.1)) {
      screenShareFeedback = {
        text: t('screenshareTab.range'),
        kind: 'error'
      };
      return;
    }
    screenShareFeedback = null;
    screenShareSavePending = true;
    // Role-checked server-side; the confirmation arrives as a broadcast
    // screenshare-config frame which updates the store.
    setServerScreenShareMaxBitrate(mbps > 0 ? Math.round(mbps * 1_000_000) : null);
  }

  const SCREENSHARE_ERROR_CODES = new Set([
    'screenshare-permission-denied',
    'invalid-screenshare-bitrate',
    'screenshare-update-failed'
  ]);

  onMount(() =>
    onServerError(SCREENSHARE_ERROR_CODES, (text) => {
    screenShareSavePending = false;
      screenShareFeedback = { text, kind: 'error' };
    })
  );
</script>

{#if active}
  <div class="settings-section">
    <h3 class="section-title">{t('screenshareTab.screenShare')}</h3>
    <div class="setting-group">
      <span class="setting-label">{t('screenshareTab.screenShareMaxBitrate')}</span>
      <input
        type="number"
        bind:value={screenShareCapMbps}
        min="0"
        max="100"
        step="0.5"
      />
      <div class="setting-description">
        {t('screenshareTab.capInMbpsApplied')}
      </div>
      <div>
        <button
          class="btn btn-primary"
          onclick={saveScreenShareCap}
          disabled={!screenShareCapDirty}
        >{t('screenshareTab.saveChanges')}</button>
      </div>
      {#if screenShareFeedback}
        <div class="identity-feedback" class:error={screenShareFeedback.kind === 'error'}>
          {screenShareFeedback.text}
        </div>
      {/if}
    </div>
  </div>
{/if}
