<!--
  Reminders and scheduled messages: everything the server is holding on this
  account's behalf, in one place.

  Both lists are the server's answer, cached (`stores/scheduled.ts`). Nothing
  here mutates them directly — a dismiss or a cancel sends the frame and waits
  for the snapshot that comes back, so two open clients cannot disagree about
  what is still queued.
-->
<script lang="ts">
  import { chat } from '$lib/stores/chat';
  import { scheduled } from '$lib/stores/scheduled';
  import { channels } from '$lib/stores/channels';
  import { onDestroy } from 'svelte';
  import { describeWhen, describeScheduleFailure } from '$lib/chat/schedule';

  interface Props {
    open: boolean;
    close: () => void;
    /** Jump to the message a reminder was set on. */
    onOpenMessage: (channelId: number, messageId: number) => void;
  }

  let { open, close, onOpenMessage }: Props = $props();

  /* "in 20 min" has to keep counting down while the panel is open, so the
     relative labels are recomputed against a ticking `now`. Half a minute is
     as fine-grained as any label here gets. */
  const TICK_MS = 30_000;
  let now = $state(new Date());
  let ticker: ReturnType<typeof setInterval> | null = null;

  function stopTicking() {
    if (ticker !== null) {
      clearInterval(ticker);
      ticker = null;
    }
  }

  $effect(() => {
    if (!open) {
      stopTicking();
      return;
    }
    chat.refreshScheduled();
    now = new Date();
    ticker ??= setInterval(() => {
      now = new Date();
    }, TICK_MS);
  });

  onDestroy(stopTicking);

  let reminders = $derived($scheduled.reminders);
  let messages = $derived($scheduled.messages);

  function channelName(id: number): string {
    return $channels.find((channel) => channel.id === id)?.name ?? 'a deleted channel';
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') close();
  }

  function handleOverlayKeydown(event: KeyboardEvent) {
    // Keyboard activation of the focused backdrop only; keystrokes inside the
    // modal content bubble here and must not close it.
    if (event.target !== event.currentTarget) return;
    if (event.key === 'Enter' || event.key === ' ') close();
  }

  function jump(channelId: number | null, messageId: number | null) {
    if (channelId === null || messageId === null) return;
    onOpenMessage(channelId, messageId);
    close();
  }
</script>

{#if open}
  <div
    class="modal-overlay"
    onclick={close}
    onkeydown={handleOverlayKeydown}
    role="dialog"
    aria-modal="true"
    aria-labelledby="schedule-panel-title"
    tabindex="-1"
  >
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      class="modal-content"
      onclick={(event) => event.stopPropagation()}
      onkeydown={handleKeydown}
      role="document"
      tabindex="0"
    >
      <div class="modal-header">
        <h2 id="schedule-panel-title">Reminders &amp; scheduled messages</h2>
        <button class="icon-btn close-btn" onclick={close} aria-label="Close reminders">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>

      <div class="modal-body">
        <section>
          <h3>Reminders</h3>
          {#if reminders.length === 0}
            <p class="empty">
              Nothing set. <code>/remind 15m take a break</code>, or use “Remind me about this”
              on any message.
            </p>
          {:else}
            <ul>
              {#each reminders as reminder (reminder.id)}
                <li class:due={reminder.firedAt !== null}>
                  <div class="row-main">
                    <span class="row-text">{reminder.text}</span>
                    <span class="row-meta">
                      {#if reminder.firedAt !== null}
                        <span class="badge">Due</span>
                      {/if}
                      {describeWhen(reminder.remindAt, now)}
                      {#if reminder.channelId !== null && reminder.messageId !== null}
                        · <button
                          type="button"
                          class="link"
                          onclick={() => jump(reminder.channelId, reminder.messageId)}
                        >
                          #{channelName(reminder.channelId)}
                        </button>
                      {/if}
                    </span>
                  </div>
                  <button
                    class="btn btn-ghost"
                    onclick={() => chat.cancelReminder(reminder.id)}
                  >
                    {reminder.firedAt !== null ? 'Dismiss' : 'Cancel'}
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </section>

        <section>
          <h3>Scheduled messages</h3>
          {#if messages.length === 0}
            <p class="empty">
              Nothing queued. <code>/schedule 2h the standup notes are up</code> posts to the
              channel you are in.
            </p>
          {:else}
            <ul>
              {#each messages as message (message.id)}
                <li class:failed={message.failedReason !== null}>
                  <div class="row-main">
                    <span class="row-text">
                      {#if message.text !== null}
                        {message.text}
                      {:else if message.encrypted}
                        <em>Encrypted — waiting for this channel’s key.</em>
                      {:else}
                        <em>No preview.</em>
                      {/if}
                    </span>
                    <span class="row-meta">
                      #{channelName(message.channelId)} · {describeWhen(message.scheduledFor, now)}
                    </span>
                    {#if message.failedReason !== null}
                      <span class="row-error">
                        {describeScheduleFailure(message.failedReason)}
                      </span>
                    {/if}
                  </div>
                  <button
                    class="btn btn-ghost"
                    onclick={() => chat.cancelScheduledMessage(message.id)}
                  >
                    {message.failedReason !== null ? 'Clear' : 'Cancel'}
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </section>
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-overlay {
    position: fixed;
    inset: 0;
    background: var(--color-overlay);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-5);
    z-index: var(--z-modal);
  }

  .modal-content {
    background: var(--color-surface-elevated);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    border: 1px solid var(--color-surface-outline);
    width: min(640px, 94vw);
    max-height: min(680px, 86vh);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-5);
    border-bottom: 1px solid var(--color-surface-outline);
  }

  .modal-header h2 {
    font-size: var(--text-lg);
  }

  .modal-body {
    flex: 1;
    min-height: 0;
    padding: var(--space-5);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }

  h3 {
    font-size: var(--text-sm);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--color-muted);
    margin: 0 0 var(--space-3);
  }

  .empty {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-sm);
  }

  .empty code {
    background: var(--color-surface-raised);
    border-radius: var(--radius-xs);
    padding: 0 var(--space-1);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  li {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--color-surface-raised);
    border: 1px solid transparent;
  }

  li.due {
    border-color: color-mix(in srgb, var(--color-primary) 40%, transparent);
  }

  li.failed {
    border-color: color-mix(in srgb, var(--color-error) 40%, transparent);
  }

  .row-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .row-text {
    overflow-wrap: anywhere;
  }

  .row-meta {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .row-error {
    font-size: var(--text-xs);
    color: var(--color-error);
  }

  .link {
    border: none;
    background: transparent;
    padding: 0;
    color: var(--color-primary);
    cursor: pointer;
    font: inherit;
  }

  .link:hover {
    text-decoration: underline;
  }
</style>
