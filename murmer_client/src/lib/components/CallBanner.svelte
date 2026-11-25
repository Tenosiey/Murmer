<!--
  A ringing direct call (`stores/calls.ts`), floating over the chat page
  wherever the user is: ringing in with Accept and Decline, ringing out with
  Cancel. It only lives while the call rings, because it covers the header;
  once answered, the call is an ordinary voice session, named in the
  sidebar's voice controls and hung up with their Leave button.
-->
<script lang="ts">
  import { call, acceptCall, endCall } from '#lib/stores/calls.ts';
  import { displayNames } from '#lib/stores/profiles.ts';
  import UserAvatar from './UserAvatar.svelte';
  import { t } from '#lib/i18n/index.ts';
</script>

{#if $call && !$call.accepted}
  {@const name = $displayNames($call.peer)}
  <div class="call-banner surface-card" role="alertdialog" aria-live="assertive">
    <UserAvatar name={$call.peer} size="sm" />
    <span class="label">
      {#if $call.outgoing}
        {t('calls.outgoing', { name })}
      {:else}
        {t('calls.incoming', { name })}
      {/if}
    </span>
    {#if !$call.outgoing}
      <button type="button" class="btn btn-primary" onclick={acceptCall}>{t('calls.accept')}</button>
      <button type="button" class="btn btn-danger" onclick={endCall}>{t('calls.decline')}</button>
    {:else}
      <button type="button" class="btn btn-danger" onclick={endCall}>{t('calls.cancel')}</button>
    {/if}
  </div>
{/if}

<style>
  .call-banner {
    position: fixed;
    top: var(--space-4);
    left: 50%;
    transform: translateX(-50%);
    z-index: var(--z-overlay);
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    box-shadow: var(--shadow-lg);
    max-width: calc(100vw - 2 * var(--space-4));
  }

  .label {
    font-size: var(--text-sm);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
