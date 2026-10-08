<script lang="ts">
  import { onMount } from 'svelte';
  import { chat, onServerError } from '$lib/stores/chat';
  import { dialogs } from '$lib/stores/dialogs';
  import { t } from '$lib/i18n';

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
      message: t('danger.typeToConfirm', { message, phrase }),
      label: t('danger.confirmation'),
      placeholder: phrase,
      confirmLabel: t('danger.continue')
    });
    if (typed === null) return false;
    if (typed.trim() !== phrase) {
      dangerFeedback = { text: t('danger.mismatch'), kind: 'error' };
      return false;
    }
    return true;
  }

  async function purgeMessages() {
    dangerFeedback = null;
    const confirmed = await confirmDestructive(
      t('danger.purgeTitle'),
      t('danger.purgeMessage'),
      'PURGE'
    );
    if (!confirmed) return;
    chat.sendRaw({ type: 'purge-all-messages', confirm: 'PURGE' });
    dangerFeedback = { text: t('danger.purgeRequested'), kind: 'info' };
  }

  async function resetServer() {
    dangerFeedback = null;
    const confirmed = await confirmDestructive(
      t('danger.resetTitle'),
      t('danger.resetMessage'),
      'RESET'
    );
    if (!confirmed) return;
    chat.sendRaw({ type: 'reset-server', confirm: 'RESET' });
    dangerFeedback = { text: t('danger.resetRequested'), kind: 'info' };
  }

  const MAINTENANCE_ERROR_CODES = new Set([
    'maintenance-permission-denied',
    'maintenance-not-confirmed',
    'maintenance-failed'
  ]);

  onMount(() =>
    onServerError(MAINTENANCE_ERROR_CODES, (text) => {
      dangerFeedback = { text, kind: 'error' };
    })
  );
</script>

{#if active}
  <div class="settings-section">
    <h3 class="section-title">{t('danger.dangerZone')}</h3>
    <div class="setting-group">
      <span class="setting-label">{t('danger.purgeAllMessages')}</span>
      <div class="setting-description">
        {t('danger.permanentlyDeleteEveryMessage')}
      </div>
      <div><button class="btn btn-danger" onclick={purgeMessages}>{t('danger.purgeMessages')}</button></div>
    </div>
    <div class="setting-group">
      <span class="setting-label">{t('danger.resetServer')}</span>
      <div class="setting-description">
        {t('danger.deleteEveryChannelExcept')} <strong>{t('danger.general')}</strong>{t('danger.plusAllCategoriesChannel')}
        <strong>{t('danger.everyone')}</strong> {t('danger.and')} <strong>{t('danger.owner')}</strong> {t('danger.thoseTwoStaySo')}
      </div>
      <div><button class="btn btn-danger" onclick={resetServer}>{t('danger.resetServer2')}</button></div>
    </div>
    {#if dangerFeedback}
      <div class="identity-feedback" class:error={dangerFeedback.kind === 'error'}>
        {dangerFeedback.text}
      </div>
    {/if}
  </div>
{/if}
