<!--
  Landing route for invite links (`https://<host>/invite#url=...`). It parses
  the link, hands the result to the `pendingInvite` store and moves on: the
  server hub renders the actual "join this server?" card, so an invite opened
  in the browser and one pasted into the desktop client's add-server field end
  up in exactly the same place. Signed-out visitors are sent through the login
  screen first, which returns them to the hub with the invite still pending.
-->
<script lang="ts">
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { parseInviteLink } from '$lib/invite';
  import { pendingInvite } from '$lib/stores/pendingInvite';
  import { session } from '$lib/stores/session';
  import MurmerLogo from '$lib/components/MurmerLogo.svelte';
  import { t } from '$lib/i18n';

  let invalid = $state(false);

  onMount(() => {
    const invite = parseInviteLink(window.location.href);
    if (!invite) {
      invalid = true;
      return;
    }
    pendingInvite.set(invite);
    goto(get(session).user ? '/servers' : '/login');
  });
</script>

<main class="invite-page">
  <div class="invite-column">
    <div class="brand">
      <MurmerLogo size={48} wordmark />
    </div>

    {#if invalid}
      <div class="surface-card invite-card" role="alert">
        <h1>{t('invite.invalidTitle')}</h1>
        <p class="body-muted">{t('invite.invalidBody')}</p>
        <a class="btn btn-primary" href="/servers">{t('invite.goToServers')}</a>
      </div>
    {:else}
      <p class="body-muted" aria-live="polite">{t('invite.opening')}</p>
    {/if}
  </div>
</main>

<style>
  .invite-page {
    min-height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-5);
  }

  .invite-column {
    width: min(400px, 100%);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-5);
    text-align: center;
  }

  .invite-card {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-6);
  }

  .invite-card h1 {
    font-size: var(--text-xl);
  }
</style>
