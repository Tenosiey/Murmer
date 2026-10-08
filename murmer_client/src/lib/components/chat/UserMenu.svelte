<!--
  Right-click menu on a member: their profile, a DM, a poke, their stats, a block
  that only this client knows about and — for moderators who outrank them —
  roles, mute, nickname, kick and ban. The
  entries only follow the server's rules so the menu does not offer what
  would be refused; the server re-checks the permission and the hierarchy on
  every one of these frames.
-->
<script lang="ts">
  import { chat } from '$lib/stores/chat';
  import { session } from '$lib/stores/session';
  import { userRoleIds } from '$lib/stores/roles';
  import { roleDefinitions } from '$lib/stores/roleDefinitions';
  import { can, myTopPosition, myPermissions } from '$lib/stores/permissions';
  import { displayNames, profiles } from '$lib/stores/profiles';
  import { blockedUsers, confirmBlock } from '$lib/stores/blocks';
  import { onlineUsers } from '$lib/stores/online';
  import { dialogs } from '$lib/stores/dialogs';
  import { t } from '$lib/i18n';
  import { poke } from '$lib/stores/pokes';
  import { PERMISSIONS, computeTopPosition } from '$lib/chat/permissions';
  import { MAX_NICKNAME_LENGTH } from '$lib/chat/constants';
  import type { ContextMenuItem } from '$lib/types';
  import ContextMenu from '$lib/components/ContextMenu.svelte';
  import UserStatsModal from '$lib/components/UserStatsModal.svelte';

  interface Props {
    onOpenProfile: (user: string) => void;
    onOpenDm: (user: string) => void;
  }

  let { onOpenProfile, onOpenDm }: Props = $props();

  let menuOpen = $state(false);
  let menuX = $state(0);
  let menuY = $state(0);
  let target: string | null = $state(null);
  let statsUser: string | null = $state(null);

  /** Open the menu for `user`; there is none for yourself. */
  export function open(event: MouseEvent, user: string) {
    if (user === $session.user) return;
    event.preventDefault();
    event.stopPropagation();
    menuX = event.clientX;
    menuY = event.clientY;
    target = user;
    menuOpen = true;
  }

  /** Highest hierarchy position of a target user (Infinity for admins). */
  function targetTopPosition(user: string): number {
    return computeTopPosition($roleDefinitions, $userRoleIds[user] ?? []);
  }

  /** Whether the current user strictly outranks `user`. */
  function outranks(user: string): boolean {
    if (user === $session.user) return false;
    return $myTopPosition > targetTopPosition(user);
  }

  /** Replace a user's assigned roles (server validates the hierarchy). */
  function setUserRoles(user: string, roleIds: number[]) {
    chat.sendRaw({ type: 'set-user-roles', user, roleIds });
  }

  /** Toggle a single role on a user, preserving their other assignments. */
  function toggleUserRole(user: string, roleId: number) {
    const current = $userRoleIds[user] ?? [];
    const next = current.includes(roleId)
      ? current.filter((id) => id !== roleId)
      : [...current, roleId];
    setUserRoles(user, next);
  }

  function kickUser(user: string) {
    chat.sendRaw({ type: 'kick-user', user });
  }

  async function banUser(user: string) {
    const confirmed = await dialogs.confirm({
      title: t('userMenu.banTitle', { name: user }),
      message: t('userMenu.banMessage'),
      confirmLabel: t('userMenu.banConfirm'),
      danger: true
    });
    if (!confirmed) return;
    chat.sendRaw({ type: 'ban-user', user });
  }

  function unbanUser(user: string) {
    chat.sendRaw({ type: 'unban-user', user });
  }

  function muteUser(user: string, durationSeconds?: number) {
    const payload: Record<string, unknown> = { type: 'mute-user', user };
    if (typeof durationSeconds === 'number') payload.durationSeconds = durationSeconds;
    chat.sendRaw(payload);
  }

  function unmuteUser(user: string) {
    chat.sendRaw({ type: 'unmute-user', user });
  }

  /** Set or clear another member's nickname on this server. */
  async function changeNicknamePrompt(user: string) {
    const current = $profiles[user]?.nickname ?? '';
    const nickname = await dialogs.prompt({
      title: t('userMenu.nicknameTitle', { name: user }),
      message: t('userMenu.nicknameMessage'),
      label: t('userMenu.nickname'),
      initial: current,
      maxLength: MAX_NICKNAME_LENGTH,
      confirmLabel: t('userMenu.save'),
      required: false
    });
    // `null` is a cancelled dialog; an empty string is a deliberate clear.
    if (nickname === null) return;
    profiles.setNickname(user, nickname.trim());
  }

  // Roles the current user may grant: below their own position and no more
  // powerful than themselves (the server enforces the same bounds).
  let assignableRoles = $derived(
    $roleDefinitions.filter(
      (def) =>
        !def.isDefault &&
        def.position < $myTopPosition &&
        ($can(PERMISSIONS.ADMINISTRATOR) || (def.permissions & ~$myPermissions) === 0)
    )
  );

  let items = $derived.by(() => {
    if (!target) return [];
    const user = target;
    const items: ContextMenuItem[] = [];
    items.push({ label: t('userMenu.viewProfile'), action: () => onOpenProfile(user) });
    items.push({ label: t('userMenu.sendMessage'), action: () => onOpenDm(user) });
    if ($onlineUsers.includes(user)) items.push({ label: t('userMenu.poke'), action: () => poke(user) });
    items.push({ label: t('userMenu.viewStats'), action: () => (statsUser = user) });
    items.push(
      $blockedUsers.includes(user)
        ? { label: t('userMenu.unblock'), action: () => blockedUsers.setBlocked(user, false) }
        : { label: t('userMenu.block'), danger: true, action: () => confirmBlock(user, $displayNames(user)) }
    );
    // Role assignment: a checklist of grantable roles, shown only to managers
    // who outrank the target.
    if ($can(PERMISSIONS.MANAGE_ROLES) && outranks(user) && assignableRoles.length) {
      const assigned = new Set($userRoleIds[user] ?? []);
      const roleItems: ContextMenuItem[] = assignableRoles.map((def) => ({
        label: assigned.has(def.id) ? `${def.name} (assigned)` : def.name,
        action: () => toggleUserRole(user, def.id)
      }));
      items.push({ label: t('userMenu.roles'), children: roleItems });
    }
    if (outranks(user)) {
      if ($can(PERMISSIONS.MUTE_MEMBERS)) {
        items.push({
          label: t('userMenu.mute'),
          children: [
            { label: t('userMenu.mute10m'), action: () => muteUser(user, 600) },
            { label: t('userMenu.mute1h'), action: () => muteUser(user, 3600) },
            { label: t('userMenu.muteUntilLifted'), action: () => muteUser(user) },
            { label: t('userMenu.unmute'), action: () => unmuteUser(user) }
          ]
        });
      }
      if ($can(PERMISSIONS.MANAGE_NICKNAMES)) {
        items.push({ label: t('userMenu.changeNickname'), action: () => changeNicknamePrompt(user) });
      }
      if ($can(PERMISSIONS.KICK_MEMBERS) && $onlineUsers.includes(user)) {
        items.push({ label: t('userMenu.kick'), danger: true, action: () => kickUser(user) });
      }
      if ($can(PERMISSIONS.BAN_MEMBERS)) {
        items.push({ label: t('userMenu.ban'), danger: true, action: () => banUser(user) });
        items.push({ label: t('userMenu.unban'), action: () => unbanUser(user) });
      }
    }
    return items;
  });
</script>

<ContextMenu bind:open={menuOpen} x={menuX} y={menuY} {items} />
<UserStatsModal open={statsUser !== null} user={statsUser} close={() => (statsUser = null)} />
