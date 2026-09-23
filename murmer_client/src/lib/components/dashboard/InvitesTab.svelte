<script lang="ts">
  import { onMount, onDestroy, untrack } from 'svelte';
  import { chat } from '$lib/stores/chat';
  import { selectedServer } from '$lib/stores/servers';
  import { dialogs } from '$lib/stores/dialogs';
  import { describeServerError } from '$lib/errors';
  import { displayNames } from '$lib/stores/profiles';
  import { invites, inviteSpent, type InviteEntry } from '$lib/stores/invites';
  import { servers } from '$lib/stores/servers';
  import { createInviteLink } from '$lib/invite';
  import { isWebClient } from '$lib/platform';
  import type { Message } from '$lib/types';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  /** Lifetimes offered by the picker, in seconds. 0 is "never expires". */
  const INVITE_EXPIRY_OPTIONS = [
    { seconds: 30 * 60, label: '30 minutes' },
    { seconds: 6 * 60 * 60, label: '6 hours' },
    { seconds: 24 * 60 * 60, label: '1 day' },
    { seconds: 7 * 24 * 60 * 60, label: '7 days' },
    { seconds: 30 * 24 * 60 * 60, label: '30 days' },
    { seconds: 0, label: 'Never' }
  ];
  /** Use limits offered by the picker. 0 is "no limit". */
  const INVITE_USE_OPTIONS = [1, 5, 10, 25, 50, 100, 0];

  let inviteExpiry = $state(24 * 60 * 60);
  let inviteMaxUses = $state(1);
  let inviteFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);
  /** Which code's link was last copied, for the button's transient label. */
  let copiedInvite: string | null = $state(null);
  let inviteCopyTimeout: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    if (active) {
      untrack(() => {
        inviteFeedback = null;
        invites.refresh();
      });
    }
  });

  function describeInviteUses(invite: InviteEntry): string {
    return invite.maxUses > 0 ? `${invite.uses}/${invite.maxUses} uses` : `${invite.uses} uses`;
  }

  function describeInviteExpiry(invite: InviteEntry): string {
    if (!invite.expiresAt) return 'never expires';
    const parsed = new Date(invite.expiresAt);
    if (Number.isNaN(parsed.getTime())) return 'unknown expiry';
    return `${parsed <= new Date() ? 'expired' : 'expires'} ${parsed.toLocaleString()}`;
  }

  /**
   * The shareable link for a code. It points at the web client we are running
   * in, or — in the desktop app, which has no origin of its own — at the
   * server, which is where a server started with `WEB_CLIENT_DIR` serves the
   * client from.
   */
  function inviteLink(code: string): string | null {
    const url = $selectedServer;
    if (!url) return null;
    const entry = servers.get(url) ?? { url, name: url };
    return createInviteLink(entry, isWebClient ? location.origin : undefined, code);
  }

  async function copyInviteLink(code: string) {
    const link = inviteLink(code);
    if (!link) return;
    try {
      await navigator.clipboard.writeText(link);
      copiedInvite = code;
      if (inviteCopyTimeout) clearTimeout(inviteCopyTimeout);
      inviteCopyTimeout = setTimeout(() => {
        if (copiedInvite === code) copiedInvite = null;
        inviteCopyTimeout = null;
      }, 2000);
    } catch (err) {
      inviteFeedback = { text: 'Could not copy the link. Copy the code instead.', kind: 'error' };
      if (import.meta.env.DEV) {
        console.error('Failed to copy invite link', err);
      }
    }
  }

  function createInvite() {
    inviteFeedback = null;
    invites.create(inviteExpiry, inviteMaxUses);
  }

  async function revokeInvite(code: string) {
    const ok = await dialogs.confirm({
      title: 'Revoke this invite?',
      message:
        'The link stops working immediately. Members who already joined through it keep their access — remove one of those with a ban.',
      confirmLabel: 'Revoke'
    });
    if (!ok) return;
    inviteFeedback = null;
    invites.revoke(code);
  }

  const INVITE_ERROR_CODES = new Set([
    'invite-permission-denied',
    'invalid-invite-options',
    'invite-limit-reached',
    'invite-not-found',
    'invite-update-failed'
  ]);

  function handleServerError(msg: Message) {
    const code = msg.message;
    if (typeof code !== 'string') return;
    if (INVITE_ERROR_CODES.has(code)) {
      inviteFeedback = { text: describeServerError(code), kind: 'error' };
    }
  }

  onMount(() => chat.on('error', handleServerError));
  onDestroy(() => chat.off('error', handleServerError));
</script>

{#if active}
  <div class="settings-section">
    <h3 class="section-title">Invites</h3>
    <div class="setting-group">
      <div class="setting-description">
        An invite link lets somebody join without being told the server password. Each
        one can expire and can be limited to a number of uses, so a link that leaks can
        be withdrawn on its own — changing the password for everybody is not the only
        way out. Revoking an invite stops new joins; members who already used it stay,
        and are removed with a ban.
      </div>
    </div>

    <div class="setting-group">
      <span class="setting-label">Create an invite</span>
      <div class="invite-form">
        <label class="invite-field">
          <span>Expires after</span>
          <select bind:value={inviteExpiry}>
            {#each INVITE_EXPIRY_OPTIONS as option (option.seconds)}
              <option value={option.seconds}>{option.label}</option>
            {/each}
          </select>
        </label>
        <label class="invite-field">
          <span>Max uses</span>
          <select bind:value={inviteMaxUses}>
            {#each INVITE_USE_OPTIONS as uses (uses)}
              <option value={uses}>{uses === 0 ? 'No limit' : uses}</option>
            {/each}
          </select>
        </label>
        <button class="btn btn-primary" onclick={createInvite}>Create invite</button>
      </div>
      {#if inviteFeedback}
        <div class="identity-feedback" class:error={inviteFeedback.kind === 'error'}>
          {inviteFeedback.text}
        </div>
      {/if}
    </div>

    <div class="setting-group">
      <span class="setting-label">Active invites</span>
      {#if $invites === null}
        <div class="setting-description">Loading…</div>
      {:else if $invites.length === 0}
        <div class="setting-description">No invites yet.</div>
      {:else}
        <ul class="invite-list">
          {#each $invites as invite (invite.code)}
            <li class="invite-row" class:spent={inviteSpent(invite)}>
              <div class="invite-identity">
                <code class="invite-code">{invite.code}</code>
                <span class="invite-meta">
                  {describeInviteUses(invite)} · {describeInviteExpiry(invite)}{invite.createdBy
                    ? ` · by ${$displayNames(invite.createdBy)}`
                    : ''}
                </span>
              </div>
              <button class="btn" onclick={() => copyInviteLink(invite.code)}>
                {copiedInvite === invite.code ? 'Copied!' : 'Copy link'}
              </button>
              <button class="btn" onclick={() => revokeInvite(invite.code)}>Revoke</button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>
{/if}

<style>
  .invite-form {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: var(--space-3);
  }

  .invite-field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .invite-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2);
    border-radius: var(--radius-sm);
  }

  .invite-row:hover {
    background: var(--color-surface-raised);
  }

  /* An expired or used-up invite is kept in the list so it can be cleared
     away deliberately, but it should not read as one that still works. */
  .invite-row.spent .invite-code,
  .invite-row.spent .invite-meta {
    opacity: 0.55;
    text-decoration: line-through;
  }

  .invite-identity {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
    margin-right: auto;
  }

  .invite-code {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    color: var(--color-on-surface);
    overflow-wrap: anywhere;
  }

  .invite-meta {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }
</style>
