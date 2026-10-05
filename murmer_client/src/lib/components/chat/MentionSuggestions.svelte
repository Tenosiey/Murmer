<!--
  Mention completion for a message field. Typing `@` lists members by the
  name the UI shows them as; picking one inserts their account name, which is
  the only thing a mention notifies by (see `$lib/chat/mentions.ts`). Members
  who may ping groups also get `@here` and the roles, listed first.

  Attaches to any input or textarea the parent passes as `field`, and anchors
  to the nearest positioned ancestor, just above it. The parent forwards its
  keydown first: `handleKey` returns true when the list consumed the key, so
  Enter picks a member instead of sending while the list is open.
-->
<script lang="ts">
  import { tick } from 'svelte';
  import { onlineUsers } from '$lib/stores/online';
  import { offlineUsers } from '$lib/stores/users';
  import { displayNames } from '$lib/stores/profiles';
  import { session } from '$lib/stores/session';
  import { roleDefinitions } from '$lib/stores/roleDefinitions';
  import { can } from '$lib/stores/permissions';
  import { PERMISSIONS } from '$lib/chat/permissions';
  import {
    MAX_MENTION_SUGGESTIONS,
    groupCandidates,
    insertMention,
    mentionCandidates,
    mentionQuery,
    type MentionCandidate
  } from '$lib/chat/mentions';
  import UserAvatar from '$lib/components/UserAvatar.svelte';

  interface Props {
    field: HTMLInputElement | HTMLTextAreaElement | undefined;
    value: string;
  }

  let { field, value = $bindable() }: Props = $props();

  const listId = $props.id();

  let caret = $state(0);
  let focused = $state(false);
  let active = $state(0);
  /* Start of a mention the user closed with Escape. It stays closed while
     they keep typing that mention, and reopens for the next one. */
  let dismissedAt: number | null = $state(null);

  let mention = $derived(focused ? mentionQuery(value, caret) : null);
  // Groups are only offered to who may ping them; the server would refuse
  // the whole message otherwise, and an offer that ends in a refusal is a trap.
  let candidates = $derived(
    mention
      ? [
          ...($can(PERMISSIONS.MENTION_GROUPS)
            ? groupCandidates(mention.query, $roleDefinitions)
            : []),
          ...mentionCandidates(
            mention.query,
            $onlineUsers,
            $offlineUsers,
            $displayNames,
            $session.user
          )
        ].slice(0, MAX_MENTION_SUGGESTIONS)
      : []
  );
  let open = $derived(mention !== null && mention.start !== dismissedAt && candidates.length > 0);
  let activeIndex = $derived(Math.min(active, candidates.length - 1));

  $effect(() => {
    const node = field;
    if (!node) return;
    const syncCaret = () => {
      caret = node.selectionStart ?? node.value.length;
    };
    const onInput = () => {
      syncCaret();
      active = 0;
      if (mentionQuery(node.value, caret)?.start !== dismissedAt) dismissedAt = null;
    };
    const onFocus = () => {
      focused = true;
      syncCaret();
    };
    const onBlur = () => {
      focused = false;
    };
    node.addEventListener('input', onInput);
    node.addEventListener('keyup', syncCaret);
    node.addEventListener('click', syncCaret);
    node.addEventListener('focus', onFocus);
    node.addEventListener('blur', onBlur);
    focused = document.activeElement === node;
    return () => {
      node.removeEventListener('input', onInput);
      node.removeEventListener('keyup', syncCaret);
      node.removeEventListener('click', syncCaret);
      node.removeEventListener('focus', onFocus);
      node.removeEventListener('blur', onBlur);
    };
  });

  // The field is the combobox; screen readers follow the highlighted member
  // through aria-activedescendant while focus stays in the text.
  $effect(() => {
    const node = field;
    if (!node) return;
    node.setAttribute('aria-autocomplete', 'list');
    node.setAttribute('aria-controls', listId);
    node.setAttribute('aria-expanded', String(open));
    if (open) node.setAttribute('aria-activedescendant', `${listId}-${activeIndex}`);
    else node.removeAttribute('aria-activedescendant');
  });

  async function pick(candidate: MentionCandidate) {
    if (!mention || !field) return;
    const result = insertMention(value, mention, caret, candidate.insert);
    value = result.text;
    caret = result.caret;
    await tick();
    field.focus();
    field.setSelectionRange(result.caret, result.caret);
  }

  /** Handle a keydown from the field; true when the list consumed it. */
  export function handleKey(event: KeyboardEvent): boolean {
    if (!open || !mention || event.isComposing) return false;
    switch (event.key) {
      case 'ArrowDown':
        active = (activeIndex + 1) % candidates.length;
        break;
      case 'ArrowUp':
        active = (activeIndex - 1 + candidates.length) % candidates.length;
        break;
      case 'Enter':
      case 'Tab':
        void pick(candidates[activeIndex]);
        break;
      case 'Escape':
        dismissedAt = mention.start;
        break;
      default:
        return false;
    }
    event.preventDefault();
    return true;
  }
</script>

{#if open}
  <ul class="mentions menu-panel" id={listId} role="listbox" aria-label="Mention someone">
    {#each candidates as candidate, index (`${candidate.kind}:${candidate.insert}`)}
      <!-- mousedown, not click: a click would blur the field first and close
           the list before the pick lands. Keyboard picks go via handleKey. -->
      <li
        id={`${listId}-${index}`}
        role="option"
        aria-selected={index === activeIndex}
        class:active={index === activeIndex}
        onmousedown={(event) => {
          event.preventDefault();
          void pick(candidate);
        }}
        onmouseenter={() => (active = index)}
      >
        {#if candidate.kind === 'member'}
          <UserAvatar name={candidate.insert} size="sm" />
          <span class="label">{candidate.label}</span>
          {#if candidate.label !== candidate.insert}
            <span class="hint">@{candidate.insert}</span>
          {/if}
        {:else}
          <span class="group-mark" aria-hidden="true">
            {#if candidate.kind === 'here'}
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M22 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/></svg>
            {:else}
              <span class="role-dot" style:background={candidate.color ?? 'var(--color-muted)'}></span>
            {/if}
          </span>
          <span class="label">@{candidate.label}</span>
          <span class="hint">{candidate.kind === 'here' ? 'Everyone online' : 'Role'}</span>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .mentions {
    position: absolute;
    left: 0;
    bottom: calc(100% + var(--space-1));
    z-index: var(--z-dropdown);
    min-width: 220px;
    max-width: 100%;
    margin: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-xs);
    cursor: pointer;
    color: var(--color-on-surface);
    font-size: var(--text-md);
  }

  li.active {
    background: var(--color-surface-raised);
  }

  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Same footprint as a small avatar, so labels line up down the list. */
  .group-mark {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--space-5);
    height: var(--space-5);
    color: var(--color-muted);
  }

  .role-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }

  .hint {
    margin-left: auto;
    padding-left: var(--space-2);
    color: var(--color-muted);
    font-size: var(--text-sm);
    white-space: nowrap;
  }
</style>
