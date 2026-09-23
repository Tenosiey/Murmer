<script lang="ts">
  import { onMount, onDestroy, untrack } from 'svelte';
  import { chat } from '$lib/stores/chat';
  import { describeServerError } from '$lib/errors';
  import { displayNames } from '$lib/stores/profiles';
  import { auditLog } from '$lib/stores/auditLog';
  import {
    auditActionLabel,
    auditTargetIsMember,
    AUDIT_ACTOR_ADMIN_TOKEN
  } from '$lib/chat/audit';
  import type { Message } from '$lib/types';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  // A record rather than live state: it is fetched when the tab opens and on
  // demand, never kept in step with events. Nothing here is editable — the
  // entries are written by the server as the actions happen.
  let auditFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);

  $effect(() => {
    if (active) {
      untrack(() => {
        auditFeedback = null;
        auditLog.refresh();
      });
    }
  });

  function refreshAuditLog() {
    auditFeedback = null;
    auditLog.refresh();
  }

  /** Full date and time: an audit entry's value is knowing exactly when. */
  function formatAuditDate(value: string | null): string {
    if (!value) return 'unknown time';
    const parsed = new Date(value);
    return Number.isNaN(parsed.getTime()) ? 'unknown time' : parsed.toLocaleString();
  }

  /**
   * How to label whoever acted. The `/role` endpoint's sentinel is not an
   * account, so it must skip the nickname lookup — that lookup would leave it
   * alone today, but a member named after it is exactly the confusion the
   * sentinel's parentheses exist to prevent.
   */
  function auditActorLabel(actor: string): string {
    if (!actor) return 'unknown';
    if (actor === AUDIT_ACTOR_ADMIN_TOKEN) return actor;
    return $displayNames(actor);
  }

  /**
   * A target is only run through the nickname lookup when the action says it
   * is a member. A role or a channel that shares a name with somebody would
   * otherwise be relabelled as that person.
   */
  function auditTargetLabel(action: string, target: string): string {
    return auditTargetIsMember(action) ? $displayNames(target) : target;
  }

  const AUDIT_ERROR_CODES = new Set(['audit-log-permission-denied', 'audit-log-failed']);

  function handleServerError(msg: Message) {
    const code = msg.message;
    if (typeof code !== 'string') return;
    if (AUDIT_ERROR_CODES.has(code)) {
      auditFeedback = { text: describeServerError(code), kind: 'error' };
    }
  }

  onMount(() => chat.on('error', handleServerError));
  onDestroy(() => chat.off('error', handleServerError));
</script>

{#if active}
  <div class="settings-section">
    <h3 class="section-title">Audit Log</h3>
    <div class="setting-group">
      <div class="setting-description">
        Who kicked, banned or muted a member, who changed a role or a channel's
        permissions, and who ran a Danger Zone action. Written by the server as each
        action succeeds — a refused action is not an action and leaves no entry. The log
        survives a server reset on purpose, and the oldest entries are dropped once it
        fills up.
      </div>
      <div class="rule-actions">
        <button class="btn" onclick={refreshAuditLog}>Refresh</button>
        {#if $auditLog !== null}
          <span class="setting-description">
            {$auditLog.length}
            {$auditLog.length === 1 ? 'entry' : 'entries'}
          </span>
        {/if}
      </div>
      {#if $auditLog === null}
        <div class="setting-description">Loading…</div>
      {:else if $auditLog.length === 0}
        <div class="setting-description">Nothing has been recorded yet.</div>
      {:else}
        <ul class="audit-list">
          {#each $auditLog as entry (entry.id)}
            <li class="audit-row">
              <span class="audit-action">{auditActionLabel(entry.action)}</span>
              <span class="audit-meta">
                {auditActorLabel(entry.actor)}{entry.target
                  ? ` → ${auditTargetLabel(entry.action, entry.target)}`
                  : ''}{entry.detail ? ` · ${entry.detail}` : ''}
              </span>
              <span class="audit-time">{formatAuditDate(entry.at)}</span>
            </li>
          {/each}
        </ul>
      {/if}
      {#if auditFeedback}
        <div class="identity-feedback" class:error={auditFeedback.kind === 'error'}>
          {auditFeedback.text}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  /* The log is dense and unbounded in width — the timestamp is pushed to the
     end so the eye can scan down it, and the middle column takes the slack.
     Rows carry the raised background all the time rather than on hover: they
     are read, not acted on, and the separation is what keeps a wrapped entry
     from running into the next one. */
  .audit-row {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    background: var(--color-surface-raised);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
  }

  .audit-action {
    color: var(--color-on-surface);
    font-size: var(--text-sm);
    flex-shrink: 0;
  }

  .audit-meta {
    font-size: var(--text-xs);
    color: var(--color-muted);
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .audit-time {
    font-size: var(--text-xs);
    color: var(--color-muted);
    flex-shrink: 0;
  }
</style>
