<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { chat } from '$lib/stores/chat';
  import { dialogs } from '$lib/stores/dialogs';
  import { describeServerError } from '$lib/errors';
  import type { Message } from '$lib/types';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  let dangerFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);

  /**
   * Both Danger Zone actions are irreversible, so the phrase typed here is
   * also what the server requires in the frame — a stray click cannot wipe a
   * server, and neither can a frame replayed without it.
   */
  async function confirmDestructive(
    title: string,
    message: string,
    phrase: string
  ): Promise<boolean> {
    const typed = await dialogs.prompt({
      title,
      message: `${message}\n\nType ${phrase} to confirm.`,
      label: 'Confirmation',
      placeholder: phrase,
      confirmLabel: 'Continue'
    });
    if (typed === null) return false;
    if (typed.trim() !== phrase) {
      dangerFeedback = { text: 'That did not match — nothing was changed.', kind: 'error' };
      return false;
    }
    return true;
  }

  async function purgeMessages() {
    dangerFeedback = null;
    const confirmed = await confirmDestructive(
      'Purge all messages',
      'Every message, pin and reaction on this server is deleted for everyone. This cannot be undone.',
      'PURGE'
    );
    if (!confirmed) return;
    chat.sendRaw({ type: 'purge-all-messages', confirm: 'PURGE' });
    dangerFeedback = { text: 'Purge requested.', kind: 'info' };
  }

  async function resetServer() {
    dangerFeedback = null;
    const confirmed = await confirmDestructive(
      'Reset server',
      'Every channel except general, plus all categories, custom roles, wiki pages and messages are deleted. Members, bans and emojis are kept. This cannot be undone.',
      'RESET'
    );
    if (!confirmed) return;
    chat.sendRaw({ type: 'reset-server', confirm: 'RESET' });
    dangerFeedback = { text: 'Reset requested.', kind: 'info' };
  }

  const MAINTENANCE_ERROR_CODES = new Set([
    'maintenance-permission-denied',
    'maintenance-not-confirmed',
    'maintenance-failed'
  ]);

  function handleServerError(msg: Message) {
    const code = msg.message;
    if (typeof code !== 'string') return;
    if (MAINTENANCE_ERROR_CODES.has(code)) {
      dangerFeedback = { text: describeServerError(code), kind: 'error' };
    }
  }

  onMount(() => chat.on('error', handleServerError));
  onDestroy(() => chat.off('error', handleServerError));
</script>

{#if active}
  <div class="settings-section">
    <h3 class="section-title">Danger Zone</h3>
    <div class="setting-group">
      <span class="setting-label">Purge all messages</span>
      <div class="setting-description">
        Permanently delete every message, pin and reaction on this server, in every
        channel, for everyone. Channels, members and uploads are kept — the files
        behind deleted attachments stay on disk. This cannot be undone.
      </div>
      <div><button class="btn btn-danger" onclick={purgeMessages}>Purge messages…</button></div>
    </div>
    <div class="setting-group">
      <span class="setting-label">Reset server</span>
      <div class="setting-description">
        Delete every channel except <strong>general</strong>, plus all categories,
        channel permission overrides, wiki pages, messages and every role other than
        <strong>@everyone</strong> and <strong>Owner</strong> — those two stay so the
        server still has an administrator. Members, bans, emojis, sounds and recorded
        stats are kept. This cannot be undone.
      </div>
      <div><button class="btn btn-danger" onclick={resetServer}>Reset server…</button></div>
    </div>
    {#if dangerFeedback}
      <div class="identity-feedback" class:error={dangerFeedback.kind === 'error'}>
        {dangerFeedback.text}
      </div>
    {/if}
  </div>
{/if}
