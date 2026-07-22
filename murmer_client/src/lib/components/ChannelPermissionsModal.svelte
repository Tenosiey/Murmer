<!--
  Per-channel permission editor (private channels + listen-only members).
  Managers set View / Write-Talk overrides for @everyone, for roles, and for
  individual members. The server enforces the same rules and clamps overrides
  to View + Write/Talk; these controls are cosmetic.
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { chat } from '$lib/stores/chat';
  import { roleDefinitions } from '$lib/stores/roleDefinitions';
  import { channelOverrides, overridesKey } from '$lib/stores/channelOverrides';
  import { dialogs } from '$lib/stores/dialogs';
  import { describeServerError } from '$lib/errors';
  import {
    PERMISSIONS,
    overrideState,
    applyOverrideState,
    type OverrideState
  } from '$lib/chat/permissions';
  import type { Message } from '$lib/types';

  interface Props {
    open: boolean;
    close: () => void;
    channelId: number | null;
    voice: boolean;
    channelName: string;
  }

  let { open, close, channelId, voice, channelName }: Props = $props();

  let feedback: string | null = $state(null);
  let requestedKey: string | null = $state(null);

  // Fetch the channel's overrides when the modal opens.
  $effect(() => {
    if (open && channelId !== null) {
      const key = overridesKey(voice, channelId);
      if (requestedKey !== key) {
        requestedKey = key;
        feedback = null;
        channelOverrides.request(channelId, voice);
      }
    } else if (!open) {
      requestedKey = null;
    }
  });

  let entry = $derived(
    channelId !== null ? $channelOverrides[overridesKey(voice, channelId)] : undefined
  );
  let overrides = $derived(entry?.overrides ?? []);

  const WRITE_FLAG = PERMISSIONS.SEND_MESSAGES;
  const VIEW_FLAG = PERMISSIONS.VIEW_CHANNELS;
  let writeLabel = $derived(voice ? 'Talk' : 'Write');

  type Target =
    | { type: 'everyone' }
    | { type: 'role'; id: number }
    | { type: 'user'; user: string };

  function pairFor(target: Target): { allow: number; deny: number } {
    const match = overrides.find((o) => {
      if (o.targetType !== target.type) return false;
      if (target.type === 'role') return o.targetId === String(target.id);
      if (target.type === 'user') return o.targetLabel === target.user;
      return true;
    });
    return match ? { allow: match.allow, deny: match.deny } : { allow: 0, deny: 0 };
  }

  function setState(target: Target, flag: number, state: OverrideState) {
    if (channelId === null) return;
    const cur = pairFor(target);
    const { allow, deny } = applyOverrideState(cur.allow, cur.deny, flag, state);
    if (allow === 0 && deny === 0) {
      channelOverrides.removeOverride(channelId, voice, target);
    } else {
      channelOverrides.setOverride(channelId, voice, target, allow, deny);
    }
  }

  let everyone = $derived(pairFor({ type: 'everyone' }));
  let isPrivate = $derived((everyone.deny & VIEW_FLAG) !== 0);

  function togglePrivate() {
    setState({ type: 'everyone' }, VIEW_FLAG, isPrivate ? 'inherit' : 'deny');
  }

  // Roles that can carry overrides: everything except the implicit @everyone
  // (shown as its own row) and the Owner (bypasses overrides).
  let editableRoles = $derived($roleDefinitions.filter((r) => !r.isDefault && !r.isOwner));

  // Member (user) overrides currently configured.
  let userOverrides = $derived(overrides.filter((o) => o.targetType === 'user'));

  async function addMember() {
    const name = await dialogs.prompt({
      title: 'Add member override',
      message: 'Grant or restrict a specific member in this channel.',
      placeholder: 'Username',
      confirmLabel: 'Add'
    });
    if (name === null) return;
    const trimmed = name.trim();
    if (!trimmed) return;
    // Seed with an explicit View allow so the override persists; adjust below.
    setState({ type: 'user', user: trimmed }, VIEW_FLAG, 'allow');
  }

  function removeMember(user: string) {
    if (channelId === null) return;
    channelOverrides.removeOverride(channelId, voice, { type: 'user', user });
  }

  function handleServerError(msg: Message) {
    if (!open) return;
    const code = (msg as any).message;
    if (
      typeof code === 'string' &&
      (code.startsWith('channel-override') ||
        code === 'invalid-channel-override' ||
        code === 'override-target-not-found')
    ) {
      feedback = describeServerError(code);
    }
  }

  onMount(() => chat.on('error', handleServerError));
  onDestroy(() => chat.off('error', handleServerError));

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') close();
  }
  function handleOverlayKeydown(event: KeyboardEvent) {
    if (event.target !== event.currentTarget) return;
    if (event.key === 'Enter' || event.key === ' ') close();
  }

  const STATES: { value: OverrideState; label: string }[] = [
    { value: 'deny', label: '✕' },
    { value: 'inherit', label: '/' },
    { value: 'allow', label: '✓' }
  ];
</script>

{#if open && channelId !== null}
  <div
    class="modal-overlay"
    onclick={close}
    onkeydown={handleOverlayKeydown}
    role="dialog"
    aria-modal="true"
    aria-labelledby="channel-perms-title"
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
        <h2 id="channel-perms-title">Permissions — {channelName}</h2>
        <button class="icon-btn close-btn" onclick={close} aria-label="Close permissions">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>

      <div class="modal-body">
        <label class="private-toggle">
          <input type="checkbox" checked={isPrivate} onchange={togglePrivate} />
          <span>
            <span class="private-label">Private channel</span>
            <span class="private-desc">
              Hide this channel from everyone by default, then grant access to the roles and
              members below.
            </span>
          </span>
        </label>

        {#if feedback}
          <div class="feedback">{feedback}</div>
        {/if}

        <div class="perm-table" role="table">
          <div class="perm-head" role="row">
            <span class="perm-target">Role / member</span>
            <span class="perm-col">View</span>
            <span class="perm-col">{writeLabel}</span>
          </div>

          <!-- @everyone -->
          {@render row('@everyone', { type: 'everyone' })}

          <!-- Roles -->
          {#each editableRoles as role (role.id)}
            {@render row(role.name, { type: 'role', id: role.id }, role.color)}
          {/each}

          <!-- Members -->
          {#each userOverrides as o (o.targetId)}
            {@render row(o.targetLabel, { type: 'user', user: o.targetLabel }, undefined, true)}
          {/each}
        </div>

        <button class="btn add-member" onclick={addMember}>+ Add member</button>
      </div>
    </div>
  </div>
{/if}

{#snippet triState(target: Target, flag: number)}
  {@const cur = pairFor(target)}
  {@const active = overrideState(cur.allow, cur.deny, flag)}
  <span class="tri" role="group">
    {#each STATES as s (s.value)}
      <button
        type="button"
        class="tri-btn"
        class:active={active === s.value}
        class:allow={s.value === 'allow'}
        class:deny={s.value === 'deny'}
        onclick={() => setState(target, flag, s.value)}
        aria-label={s.value}
      >{s.label}</button>
    {/each}
  </span>
{/snippet}

{#snippet row(label: string, target: Target, color?: string, removable = false)}
  <div class="perm-row" role="row">
    <span class="perm-target">
      {#if color}<span class="role-dot" style={`background:${color}`} aria-hidden="true"></span>{/if}
      <span class="perm-target-name">{label}</span>
      {#if removable}
        <button class="icon-btn remove-member" onclick={() => removeMember(label)} aria-label={`Remove ${label}`}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      {/if}
    </span>
    <span class="perm-col">{@render triState(target, VIEW_FLAG)}</span>
    <span class="perm-col">{@render triState(target, WRITE_FLAG)}</span>
  </div>
{/snippet}

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
    width: min(560px, 94vw);
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
    gap: var(--space-4);
  }

  .private-toggle {
    display: flex;
    gap: var(--space-3);
    align-items: flex-start;
  }

  .private-toggle span {
    display: flex;
    flex-direction: column;
  }

  .private-label {
    font-weight: 600;
  }

  .private-desc {
    font-size: var(--text-sm);
    color: var(--color-muted);
  }

  .feedback {
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    font-size: var(--text-sm);
    border: 1px solid color-mix(in srgb, var(--color-error) 35%, transparent);
    background: color-mix(in srgb, var(--color-error) 10%, transparent);
    color: var(--color-error);
  }

  .perm-table {
    display: flex;
    flex-direction: column;
  }

  .perm-head,
  .perm-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 96px 96px;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) 0;
  }

  .perm-head {
    font-size: var(--text-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--color-muted);
    border-bottom: 1px solid var(--color-surface-outline);
  }

  .perm-row {
    border-bottom: 1px solid var(--color-surface-outline);
  }

  .perm-col {
    display: flex;
    justify-content: center;
  }

  .perm-target {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .perm-target-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .role-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .remove-member {
    width: 20px;
    height: 20px;
    color: var(--color-muted);
  }

  .tri {
    display: inline-flex;
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  .tri-btn {
    width: 28px;
    height: var(--control-height-sm, 28px);
    border: none;
    background: var(--color-surface);
    color: var(--color-muted);
    cursor: pointer;
    font-size: var(--text-sm);
  }

  .tri-btn + .tri-btn {
    border-left: 1px solid var(--color-surface-outline);
  }

  .tri-btn.active {
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
  }

  .tri-btn.active.allow {
    background: color-mix(in srgb, var(--color-success) 25%, transparent);
    color: var(--color-success);
  }

  .tri-btn.active.deny {
    background: color-mix(in srgb, var(--color-error) 25%, transparent);
    color: var(--color-error);
  }

  .add-member {
    align-self: flex-start;
  }
</style>
