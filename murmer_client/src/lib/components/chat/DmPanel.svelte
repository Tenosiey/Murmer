<!--
  The open direct message, in the conversation side panel. Everything here is
  end-to-end encrypted, so beyond sending it carries the two things only a DM
  needs: accepting a peer's changed identity key, and showing the fingerprint
  to compare out of band before doing so. Opening a conversation is the chat
  page's job, since it also closes an open thread.
-->
<script lang="ts">
  import { chat } from '$lib/stores/chat';
  import { dm } from '$lib/stores/dm';
  import { session } from '$lib/stores/session';
  import { displayNames } from '$lib/stores/profiles';
  import { dmDraft } from '$lib/stores/drafts';
  import { peerKeys } from '$lib/stores/peerKeys';
  import { dialogs } from '$lib/stores/dialogs';
  import { t } from '$lib/i18n';
  import { dmFingerprint } from '$lib/dm-crypto';
  import { loadKeyPair } from '$lib/keypair';
  import ConversationPanel from './ConversationPanel.svelte';

  const conversations = dm.conversations;
  const activePeer = dm.activePeer;
  const conflicts = peerKeys.conflicts;

  let messages = $derived($activePeer ? ($conversations[$activePeer] ?? []) : []);

  function send(text: string) {
    const peer = $activePeer;
    if (!peer) return;
    void chat.sendDm(peer, text).then((error) => {
      if (error) void dialogs.alert({ title: t('dmPanel.notSent'), message: error });
    });
  }

  /** Accept the peer's changed identity key after the user confirmed it. */
  function trustKey() {
    const peer = $activePeer;
    if (peer) peerKeys.trust(peer);
  }

  /** Show the conversation's key fingerprint for out-of-band comparison. */
  function verify() {
    const peer = $activePeer;
    if (!peer) return;
    // Verify the key actually in use: the unconfirmed new key if there is
    // a conflict, the pinned one otherwise.
    const peerKey = $conflicts[peer] ?? peerKeys.pinned(peer);
    if (!peerKey) {
      void dialogs.alert({
        title: t('dmPanel.noKeyTitle'),
        message: t('dmPanel.noKeyMessage', { name: peer })
      });
      return;
    }
    void dialogs.alert({
      title: t('dmPanel.verifyTitle', { name: peer }),
      message: t('dmPanel.verifyMessage', {
        fingerprint: dmFingerprint(loadKeyPair().publicKey, peerKey),
        name: peer
      })
    });
  }
</script>

{#if $activePeer}
  <ConversationPanel
    kind="dm"
    title={$displayNames($activePeer)}
    {messages}
    emptyText={t('dmPanel.empty')}
    placeholder={t('dmPanel.placeholder', { name: $displayNames($activePeer) })}
    onSend={send}
    onClose={() => dm.close()}
    draftKey={dmDraft($activePeer)}
    emphasize={(msg) => msg.from === $session.user}
    keyWarning={$activePeer in $conflicts}
    onTrustKey={trustKey}
    onVerify={verify}
  />
{/if}
