<!--
  A poll's options under its question (the message text). Each option is a
  button: pressing one casts the reader's single vote, pressing their current
  choice takes it back. Counts come from the server's tally only — the click
  sends a vote and waits for `poll-update`, so what is shown is what was
  counted.
-->
<script lang="ts">
  import type { PollInfo } from '$lib/types';
  import { chat } from '$lib/stores/chat';
  import { session } from '$lib/stores/session';
  import { displayNames } from '$lib/stores/profiles';
  import { t } from '$lib/i18n';

  interface Props {
    messageId: number;
    poll: PollInfo;
  }

  let { messageId, poll }: Props = $props();

  let total = $derived(poll.votes.reduce((sum, voters) => sum + voters.length, 0));
  let mine = $derived(poll.votes.findIndex((voters) => voters.includes($session.user ?? '')));

  function percent(count: number): number {
    return total === 0 ? 0 : Math.round((count / total) * 100);
  }
</script>

<div class="poll" role="group" aria-label={t('pollCard.poll')}>
  {#each poll.options as option, i (i)}
    {@const voters = poll.votes[i] ?? []}
    <button
      type="button"
      class="poll-option"
      class:chosen={mine === i}
      aria-pressed={mine === i}
      title={voters.length ? voters.map((v) => $displayNames(v)).join(', ') : t('pollCard.noVotes')}
      onclick={() => chat.votePoll(messageId, mine === i ? null : i)}
    >
      <span class="poll-bar" style={`width: ${percent(voters.length)}%`} aria-hidden="true"></span>
      <span class="poll-label">{option}</span>
      <span class="poll-count">{voters.length} · {percent(voters.length)}%</span>
    </button>
  {/each}
  <span class="poll-total">
    {t('pollCard.votes', { count: total })}{mine >= 0 ? t('pollCard.retractHint') : ''}
  </span>
</div>

<style>
  .poll {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin-top: var(--space-2);
    max-width: 28rem;
  }

  .poll-option {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    overflow: hidden;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-surface-outline);
    background: var(--color-surface-elevated);
    color: var(--color-on-surface);
    font-size: var(--text-sm);
    text-align: left;
    cursor: pointer;
  }

  .poll-option:hover {
    border-color: var(--color-outline-strong);
  }

  .poll-option.chosen {
    border-color: color-mix(in srgb, var(--color-primary) 45%, transparent);
  }

  .poll-bar {
    position: absolute;
    inset: 0 auto 0 0;
    background: var(--color-primary-container);
    transition: width 0.2s ease;
  }

  .poll-label,
  .poll-count {
    position: relative;
  }

  .poll-label {
    overflow-wrap: anywhere;
  }

  .poll-count {
    flex-shrink: 0;
    color: var(--color-muted);
    font-size: var(--text-xs);
  }

  .poll-total {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }
</style>
