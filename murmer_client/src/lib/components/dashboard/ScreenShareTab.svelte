<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { chat } from '$lib/stores/chat';
  import { describeServerError } from '$lib/errors';
  import {
    screenShareServerMaxBitrate,
    setServerScreenShareMaxBitrate
  } from '$lib/stores/screenShare';
  import type { Message } from '$lib/types';

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
      screenShareFeedback = { text: 'Changes saved.', kind: 'info' };
    }
  });

  function saveScreenShareCap() {
    const mbps = Number.isFinite(screenShareCapMbps) ? screenShareCapMbps : NaN;
    if (Number.isNaN(mbps) || mbps < 0 || mbps > 100 || (mbps > 0 && mbps < 0.1)) {
      screenShareFeedback = {
        text: 'Enter a value between 0.1 and 100 Mbps, or 0 for no limit.',
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

  function handleServerError(msg: Message) {
    const code = msg.message;
    if (typeof code !== 'string') return;
    if (SCREENSHARE_ERROR_CODES.has(code)) {
      screenShareSavePending = false;
      screenShareFeedback = { text: describeServerError(code), kind: 'error' };
    }
  }

  onMount(() => chat.on('error', handleServerError));
  onDestroy(() => chat.off('error', handleServerError));
</script>

{#if active}
  <div class="settings-section">
    <h3 class="section-title">Screen Share</h3>
    <div class="setting-group">
      <span class="setting-label">Screen share max bitrate</span>
      <input
        type="number"
        bind:value={screenShareCapMbps}
        min="0"
        max="100"
        step="0.5"
      />
      <div class="setting-description">
        Cap in Mbps applied to every member's outgoing screen share; 0 means no limit.
        Screen shares travel peer-to-peer between members, so this limits member
        bandwidth use, not server load.
      </div>
      <div>
        <button
          class="btn btn-primary"
          onclick={saveScreenShareCap}
          disabled={!screenShareCapDirty}
        >Save changes</button>
      </div>
      {#if screenShareFeedback}
        <div class="identity-feedback" class:error={screenShareFeedback.kind === 'error'}>
          {screenShareFeedback.text}
        </div>
      {/if}
    </div>
  </div>
{/if}
