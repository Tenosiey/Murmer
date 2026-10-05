<!--
  Mention completion for a message field. Typing `@` lists members by the
  name the UI shows them as; picking one inserts their account name, which is
  the only thing a mention notifies by (see `$lib/chat/mentions.ts`).

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
  import {
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
  let candidates = $derived(
    mention
      ? mentionCandidates(mention.query, $onlineUsers, $offlineUsers, $displayNames, $session.user)
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
    const result = insertMention(value, mention, caret, candidate.user);
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
  <ul class="mentions menu-panel" id={listId} role="listbox" aria-label="Mention a member">
    {#each candidates as candidate, index (candidate.user)}
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
        <UserAvatar name={candidate.user} size="sm" />
        <span class="label">{candidate.label}</span>
        {#if candidate.label !== candidate.user}
          <span class="account">@{candidate.user}</span>
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

  .account {
    margin-left: auto;
    padding-left: var(--space-2);
    color: var(--color-muted);
    font-size: var(--text-sm);
    white-space: nowrap;
  }
</style>
