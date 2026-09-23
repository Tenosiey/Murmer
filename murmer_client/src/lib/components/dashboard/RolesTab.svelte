<script lang="ts">
  import { onMount, onDestroy, untrack } from 'svelte';
  import { chat } from '$lib/stores/chat';
  import { selectedServer } from '$lib/stores/servers';
  import { customEmojiList } from '$lib/stores/customEmojis';
  import { dialogs } from '$lib/stores/dialogs';
  import { describeServerError } from '$lib/errors';
  import { httpBaseFromWs } from '$lib/server-url';
  import { uploadForm, uploadErrorMessage } from '$lib/upload';
  import {
    MAX_ROLE_ICON_BYTES
  } from '$lib/chat/constants';
  import { roleDefinitions } from '$lib/stores/roleDefinitions';
  import { myTopPosition } from '$lib/stores/permissions';
  import {
    PERMISSIONS,
    PERMISSION_GROUPS
  } from '$lib/chat/permissions';
  import type { Message, RoleDef } from '$lib/types';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  let httpBase = $derived($selectedServer ? httpBaseFromWs($selectedServer) : '');

  // Roles are listed highest-power first (Owner on top, @everyone at the
  // bottom). Editing is bounded by the viewer's own position; the server
  // enforces the same rules, so these gates are cosmetic.
  let selectedRoleId: number | null = $state(null);
  let draftName = $state('');
  let draftColor = $state('');
  /** Role icon URL (`/files/<key>`), empty when the role has none. */
  let draftIcon = $state('');
  let draftPermissions = $state(0);
  let roleFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);
  let roleIconInput: HTMLInputElement | null = $state(null);
  let roleIconUploading = $state(false);

  let rolesHighToLow = $derived([...$roleDefinitions].sort((a, b) => b.position - a.position));
  let selectedRole = $derived(
    $roleDefinitions.find((r) => r.id === selectedRoleId) ?? null
  );
  // Custom (reorderable) roles, highest-power first.
  let customRolesHighToLow = $derived(
    rolesHighToLow.filter((r) => !r.isDefault && !r.isOwner)
  );

  function canManageRole(role: RoleDef): boolean {
    return $myTopPosition > role.position;
  }
  let nameEditable = $derived(!!selectedRole && canManageRole(selectedRole) && !selectedRole.isDefault);
  let permsEditable = $derived(!!selectedRole && canManageRole(selectedRole) && !selectedRole.isOwner);
  let deletable = $derived(
    !!selectedRole && canManageRole(selectedRole) && !selectedRole.isDefault && !selectedRole.isOwner
  );

  // Load the selected role into the editable draft whenever the selection (or
  // the underlying definition) changes.
  $effect(() => {
    const role = selectedRole;
    if (role) {
      untrack(() => {
        draftName = role.name;
        draftColor = role.color ?? '';
        draftIcon = role.icon ?? '';
        draftPermissions = role.permissions;
      });
    }
  });

  // Default to the highest role the viewer can see when the tab opens.
  $effect(() => {
    if (active && selectedRoleId === null && rolesHighToLow.length) {
      untrack(() => {
        selectedRoleId = rolesHighToLow[0].id;
      });
    }
  });

  function selectRole(id: number) {
    selectedRoleId = id;
    roleFeedback = null;
  }

  function togglePermission(flag: number) {
    if (!permsEditable) return;
    draftPermissions ^= flag;
  }

  function isPermissionOn(flag: number): boolean {
    if ((draftPermissions & PERMISSIONS.ADMINISTRATOR) !== 0) return true;
    return (draftPermissions & flag) === flag;
  }

  function saveRole() {
    if (!selectedRole) return;
    const color = draftColor.trim();
    const icon = draftIcon.trim();
    chat.sendRaw({
      type: 'update-role',
      id: selectedRole.id,
      name: draftName.trim(),
      color: color === '' ? null : color,
      icon: icon === '' ? null : icon,
      permissions: draftPermissions >>> 0
    });
    roleFeedback = { text: 'Changes sent.', kind: 'info' };
  }

  /**
   * Upload an image and stage it as this role's icon. Like the emoji and
   * server-icon flows the file goes through `/upload` first; the URL is only
   * registered when the role is saved, and the server re-validates it.
   */
  async function uploadRoleIcon(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0] ?? null;
    input.value = '';
    if (!file || !httpBase) return;
    roleFeedback = null;
    if (file.size > MAX_ROLE_ICON_BYTES) {
      roleFeedback = { text: 'Role icons must be 512 KB or smaller.', kind: 'error' };
      return;
    }
    roleIconUploading = true;
    try {
      const res = await fetch(httpBase + '/upload', { method: 'POST', body: uploadForm(file) });
      const uploadError = uploadErrorMessage(res.status, 'image');
      if (uploadError) {
        roleFeedback = { text: uploadError, kind: 'error' };
        return;
      }
      if (!res.ok) throw new Error(`upload failed with status ${res.status}`);
      const data = await res.json();
      if (typeof data.url !== 'string') throw new Error('upload response missing url');
      draftIcon = data.url;
      roleFeedback = { text: 'Icon ready — save to apply it.', kind: 'info' };
    } catch (e) {
      console.error('role icon upload failed', e);
      roleFeedback = { text: 'Icon upload failed. Please try again.', kind: 'error' };
    } finally {
      roleIconUploading = false;
    }
  }

  // Picking a custom emoji just reuses its uploaded image, so no second copy
  // of the file is created.
  function pickEmojiIcon(event: Event) {
    const select = event.currentTarget as HTMLSelectElement;
    const url = select.value;
    select.value = '';
    if (!url) return;
    draftIcon = url;
    roleFeedback = { text: 'Icon ready — save to apply it.', kind: 'info' };
  }

  async function createRole() {
    const name = await dialogs.prompt({
      title: 'Create role',
      message: 'Name the new role. You can set its permissions after creating it.',
      placeholder: 'e.g. Dude',
      confirmLabel: 'Create'
    });
    if (name === null) return;
    const trimmed = name.trim();
    if (!trimmed) return;
    // New roles start with the baseline view + send permissions.
    chat.sendRaw({
      type: 'create-role',
      name: trimmed,
      permissions: (PERMISSIONS.VIEW_CHANNELS | PERMISSIONS.SEND_MESSAGES) >>> 0
    });
  }

  async function deleteRole() {
    if (!selectedRole || !deletable) return;
    const confirmed = await dialogs.confirm({
      title: `Delete “${selectedRole.name}”?`,
      message: 'Members lose this role. This cannot be undone.',
      confirmLabel: 'Delete role',
      danger: true
    });
    if (!confirmed) return;
    chat.sendRaw({ type: 'delete-role', id: selectedRole.id });
    selectedRoleId = null;
  }

  // Move a custom role up (more power) or down; sends the new order to the
  // server, which re-derives positions.
  function moveRole(role: RoleDef, direction: -1 | 1) {
    const order = customRolesHighToLow.map((r) => r.id);
    const index = order.indexOf(role.id);
    const target = index + direction;
    if (index < 0 || target < 0 || target >= order.length) return;
    [order[index], order[target]] = [order[target], order[index]];
    chat.sendRaw({ type: 'reorder-roles', orderedIds: order });
  }

  const ROLE_ERROR_CODES = new Set([
    'role-permission-denied',
    'role-target-not-found',
    'role-update-failed',
    'role-not-found',
    'role-name-taken',
    'role-protected',
    'role-limit-reached',
    'invalid-role-name',
    'invalid-role-color',
    'invalid-role-icon',
    'invalid-role-permissions'
  ]);

  function handleServerError(msg: Message) {
    const code = msg.message;
    if (typeof code !== 'string') return;
    if (ROLE_ERROR_CODES.has(code)) {
      roleFeedback = { text: describeServerError(code), kind: 'error' };
    }
  }

  onMount(() => chat.on('error', handleServerError));
  onDestroy(() => chat.off('error', handleServerError));
</script>

{#if active}
  <div class="settings-section">
    <h3 class="section-title">Roles</h3>
    <div class="setting-description">
      Define roles and their permissions. Assign roles to members by
      right-clicking them in the sidebar. Everyone implicitly has
      <strong>@everyone</strong>; grant capabilities on top of it, or lower
      its baseline to restrict everyone.
    </div>

    <div class="roles-layout">
      <div class="roles-list">
        {#each rolesHighToLow as role (role.id)}
          <button
            type="button"
            class="role-row"
            class:active={role.id === selectedRoleId}
            onclick={() => selectRole(role.id)}
          >
            <span
              class="role-dot"
              style={`background: ${role.color ?? 'var(--color-muted)'}`}
              aria-hidden="true"
            ></span>
            {#if role.icon}
              <img class="role-row-icon" src={httpBase + role.icon} alt="" />
            {/if}
            <span class="role-row-name">{role.name}</span>
            {#if role.isOwner}<span class="badge">Owner</span>{/if}
            {#if role.isDefault}<span class="badge">Default</span>{/if}
          </button>
        {/each}
        <button type="button" class="btn role-create" onclick={createRole}>
          + Create role
        </button>
      </div>

      <div class="role-editor">
        {#if selectedRole}
          <div class="setting-group">
            <label class="setting-label" for="role-name">Name</label>
            <input
              id="role-name"
              class="field"
              bind:value={draftName}
              maxlength="32"
              disabled={!nameEditable}
            />
          </div>

          <div class="setting-group">
            <label class="setting-label" for="role-color">Color</label>
            <div class="role-color-row">
              <input
                id="role-color"
                class="field"
                bind:value={draftColor}
                placeholder="#3b82f6"
                disabled={!nameEditable}
              />
              <span
                class="role-dot large"
                style={`background: ${draftColor.trim() || 'var(--color-muted)'}`}
                aria-hidden="true"
              ></span>
            </div>
          </div>

          <div class="setting-group">
            <span class="setting-label">Icon</span>
            <div class="setting-description">
              Shown next to the name of every member holding this role. Pick a
              custom emoji or upload an image (PNG, JPEG, GIF or WebP, up to
              512 KB).
            </div>
            <div class="role-icon-row">
              <span class="role-icon-preview">
                {#if draftIcon}
                  <img src={httpBase + draftIcon} alt="" />
                {:else}
                  <span class="role-icon-empty">None</span>
                {/if}
              </span>
              <select
                class="field role-icon-select"
                disabled={!nameEditable || $customEmojiList.length === 0}
                onchange={pickEmojiIcon}
                aria-label="Use a custom emoji as the role icon"
              >
                <option value="">
                  {$customEmojiList.length === 0 ? 'No custom emojis' : 'Use an emoji…'}
                </option>
                {#each $customEmojiList as emoji (emoji.name)}
                  <option value={emoji.url}>:{emoji.name}:</option>
                {/each}
              </select>
              <button
                type="button"
                class="btn"
                disabled={!nameEditable || roleIconUploading}
                onclick={() => roleIconInput?.click()}
              >{roleIconUploading ? 'Uploading…' : 'Upload image…'}</button>
              {#if draftIcon}
                <button
                  type="button"
                  class="btn btn-ghost"
                  disabled={!nameEditable}
                  onclick={() => (draftIcon = '')}
                >Remove</button>
              {/if}
              <input
                bind:this={roleIconInput}
                type="file"
                accept="image/png,image/jpeg,image/gif,image/webp"
                class="sr-only"
                onchange={uploadRoleIcon}
              />
            </div>
          </div>

          <div class="setting-group">
            <span class="setting-label">Permissions</span>
            {#if selectedRole.isOwner}
              <div class="setting-description">
                The Owner role always has every permission and cannot be changed.
              </div>
            {/if}
            {#each PERMISSION_GROUPS as group (group.title)}
              <div class="perm-group">
                <div class="perm-group-title">{group.title}</div>
                {#each group.permissions as perm (perm.key)}
                  <label class="perm-row">
                    <input
                      type="checkbox"
                      checked={isPermissionOn(perm.flag)}
                      disabled={!permsEditable}
                      onchange={() => togglePermission(perm.flag)}
                    />
                    <span class="perm-text">
                      <span class="perm-label">{perm.label}</span>
                      <span class="perm-desc">{perm.description}</span>
                    </span>
                  </label>
                {/each}
              </div>
            {/each}
          </div>

          {#if !selectedRole.isDefault && !selectedRole.isOwner}
            <div class="setting-group">
              <span class="setting-label">Position</span>
              <div class="role-reorder">
                <button
                  type="button"
                  class="btn"
                  disabled={!canManageRole(selectedRole)}
                  onclick={() => selectedRole && moveRole(selectedRole, -1)}
                >Move up</button>
                <button
                  type="button"
                  class="btn"
                  disabled={!canManageRole(selectedRole)}
                  onclick={() => selectedRole && moveRole(selectedRole, 1)}
                >Move down</button>
              </div>
            </div>
          {/if}

          {#if roleFeedback}
            <div class="identity-feedback" class:error={roleFeedback.kind === 'error'}>
              {roleFeedback.text}
            </div>
          {/if}

          <div class="role-actions">
            <button
              type="button"
              class="btn btn-primary"
              disabled={!nameEditable && !permsEditable}
              onclick={saveRole}
            >Save changes</button>
            {#if deletable}
              <button type="button" class="btn btn-danger" onclick={deleteRole}>
                Delete role
              </button>
            {/if}
          </div>
        {:else}
          <div class="setting-description">Select a role to edit it.</div>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  /* ── Roles editor ──────────────────────────────────────────────────────── */
  .roles-layout {
    display: grid;
    grid-template-columns: minmax(160px, 220px) minmax(0, 1fr);
    gap: var(--space-4);
    margin-top: var(--space-3);
  }

  .roles-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    align-content: start;
  }

  .role-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-on-surface);
    cursor: pointer;
    text-align: left;
    font: inherit;
  }

  .role-row:hover {
    background: var(--color-surface-raised);
  }

  .role-row.active {
    background: var(--color-surface-raised);
    border-color: var(--color-surface-outline);
  }

  .role-row-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .role-dot {
    width: 12px;
    height: 12px;
    border-radius: var(--radius-pill, 50%);
    flex-shrink: 0;
  }

  .role-row-icon {
    width: var(--space-4);
    height: var(--space-4);
    object-fit: contain;
    flex-shrink: 0;
  }

  .role-icon-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .role-icon-preview {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--space-7);
    height: var(--space-7);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-sm);
    background: var(--color-surface-raised);
    flex-shrink: 0;
  }

  .role-icon-preview img {
    width: var(--space-5);
    height: var(--space-5);
    object-fit: contain;
  }

  .role-icon-empty {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .role-icon-select {
    width: auto;
    min-width: 10rem;
  }

  .role-dot.large {
    width: 20px;
    height: 20px;
  }

  .role-create {
    margin-top: var(--space-2);
    justify-content: center;
  }

  .role-editor {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    min-width: 0;
  }

  .role-color-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .perm-group {
    margin-top: var(--space-2);
  }

  .perm-group-title {
    font-size: var(--text-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--color-muted);
    margin-bottom: var(--space-1);
  }

  .perm-row {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-1) 0;
    cursor: pointer;
  }

  .perm-row input[disabled] {
    cursor: not-allowed;
  }

  .perm-text {
    display: flex;
    flex-direction: column;
  }

  .perm-label {
    font-size: var(--text-sm);
    color: var(--color-on-surface);
  }

  .perm-desc {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .role-reorder,
  .role-actions {
    display: flex;
    gap: var(--space-2);
  }

  @media (max-width: 640px) {
    .roles-layout {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
