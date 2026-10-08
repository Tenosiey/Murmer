<!--
  Left-hand sidebar: text/voice channel list grouped by category, the users in
  each voice channel and the voice control panel (mute, leave, screen share,
  camera).
-->
<script lang="ts">
  import { browser } from '$app/environment';
  import { SvelteSet } from 'svelte/reactivity';
  import ConnectionBars from '$lib/components/ConnectionBars.svelte';
  import ScreenShareControls from '$lib/components/ScreenShareControls.svelte';
  import WebcamControls from '$lib/components/WebcamControls.svelte';
  import SoundboardPanel from '$lib/components/SoundboardPanel.svelte';
  import RoleIcon from '$lib/components/RoleIcon.svelte';
  import { chat } from '$lib/stores/chat';
  import { channels } from '$lib/stores/channels';
  import { voiceChannels } from '$lib/stores/voiceChannels';
  import { categories } from '$lib/stores/categories';
  import { voiceUsers } from '$lib/stores/voiceUsers';
  import { voiceStats, voiceReconnecting } from '$lib/stores/voice';
  import { viaServer } from '$lib/stores/voiceTransport';
  import { session } from '$lib/stores/session';
  import { roles } from '$lib/stores/roles';
  import { displayNames } from '$lib/stores/profiles';
  import { leftSidebarWidth } from '$lib/stores/layout';
  import { microphoneMuted, outputMuted, voiceMode, isPttActive } from '$lib/stores/settings';
  import { canSpeak } from '$lib/stores/voicePermissions';
  import { speakingUsers } from '$lib/stores/voiceSpeaking';
  import { voiceMuteStates } from '$lib/stores/voiceMute';
  import { handQueue, sendHand, voiceHands } from '$lib/stores/voiceHands';
  import {
    activeScreenShares,
    screenSharePreview,
    watchedScreenShares
  } from '$lib/stores/screenShare';
  import { activeWebcams } from '$lib/stores/webcam';
  import { unread } from '$lib/stores/unread';
  import { can } from '$lib/stores/permissions';
  import { PERMISSIONS } from '$lib/chat/permissions';
  import { formatVoiceQuality, orderVoiceChannels } from '$lib/chat/helpers';
  import type { CategoryInfo, ChannelInfo, VoiceChannelInfo } from '$lib/types';
  import { t } from '$lib/i18n';

  interface Props {
    currentChatChannelId: number;
    currentVoiceChannelId: number | null;
    inVoice: boolean;
    serverStrength: number;
    onJoinChannel: (id: number) => void;
    onJoinVoiceChannel: (id: number) => void;
    onOpenChannelMenu: (event: MouseEvent, channelId?: number, voice?: boolean) => void;
    onOpenCategoryMenu: (event: MouseEvent, category: CategoryInfo) => void;
    onOpenUserVolumeMenu: (event: MouseEvent, user: string) => void;
    onViewScreenShare: (user: string) => void;
    onLeaveVoice: () => void;
    onToggleMicrophone: () => void;
    onToggleOutput: () => void;
  }

  let {
    currentChatChannelId,
    currentVoiceChannelId,
    inVoice,
    serverStrength,
    onJoinChannel,
    onJoinVoiceChannel,
    onOpenChannelMenu,
    onOpenCategoryMenu,
    onOpenUserVolumeMenu,
    onViewScreenShare,
    onLeaveVoice,
    onToggleMicrophone,
    onToggleOutput
  }: Props = $props();

  interface CategoryGroup {
    category: CategoryInfo | null;
    textChannels: ChannelInfo[];
    voiceChannels: VoiceChannelInfo[];
  }

  interface DraggedChannel {
    id: number;
    voice: boolean;
    categoryId: number | null;
  }

  // Cosmetic soundboard gates; the server re-checks both on every frame.
  let canUseSoundboard = $derived($can(PERMISSIONS.USE_SOUNDBOARD));
  let canManageSounds = $derived($can(PERMISSIONS.MANAGE_SOUNDS));
  // Cosmetic too: the server also checks that the mover outranks the member.
  let canMoveMembers = $derived($can(PERMISSIONS.MOVE_MEMBERS));
  let handRaised = $derived(
    inVoice &&
      currentVoiceChannelId !== null &&
      $voiceHands[$session.user ?? '']?.channelId === currentVoiceChannelId
  );

  const COLLAPSED_KEY = 'murmer_collapsed_categories';
  const UNCATEGORIZED_KEY = '__uncategorized';
  /* Custom MIME types so the chat page's file-drop zone (which only reacts to
     dragged files) never mistakes a channel or category drag for an upload. */
  const CHANNEL_DRAG_MIME = 'application/x-murmer-channel';
  const CATEGORY_DRAG_MIME = 'application/x-murmer-category';
  const MEMBER_DRAG_MIME = 'application/x-murmer-member';

  function loadCollapsed(): number[] {
    if (!browser) return [];
    try {
      const parsed = JSON.parse(localStorage.getItem(COLLAPSED_KEY) ?? '[]');
      return Array.isArray(parsed) ? parsed.filter((v) => typeof v === 'number') : [];
    } catch {
      return [];
    }
  }

  // A SvelteSet, because `$state` does not track the contents of a plain Set.
  const collapsedCategories = new SvelteSet<number>(loadCollapsed());
  function toggleCategory(id: number) {
    if (!collapsedCategories.delete(id)) collapsedCategories.add(id);
    if (browser) {
      localStorage.setItem(COLLAPSED_KEY, JSON.stringify([...collapsedCategories]));
    }
  }

  /* Channel and category drag & drop. The dragged item is kept in component
     state (a drag never leaves this component) while the DataTransfer payload
     only exists so the browser reports a valid drag type during `dragover`.
     Dropping on a category body appends the channel there; dropping on a
     channel of the same kind inserts before/after it; dragging a category
     header reorders categories. Permission is enforced by the server,
     mirroring the "Move to" context menu. */
  let draggedChannel: DraggedChannel | null = $state(null);
  let dragOverKey: string | null = $state(null);
  let channelDropTarget: { id: number; voice: boolean; after: boolean } | null = $state(null);
  let draggedCategoryId: number | null = $state(null);
  let categoryDropTarget: { id: number; after: boolean } | null = $state(null);

  function groupKey(group: CategoryGroup): string {
    return group.category ? String(group.category.id) : UNCATEGORIZED_KEY;
  }

  function handleChannelDragStart(
    event: DragEvent,
    channel: ChannelInfo | VoiceChannelInfo,
    voice: boolean
  ) {
    draggedChannel = { id: channel.id, voice, categoryId: channel.categoryId ?? null };
    if (!event.dataTransfer) return;
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData(CHANNEL_DRAG_MIME, String(channel.id));
  }

  function handleChannelDragEnd() {
    draggedChannel = null;
    dragOverKey = null;
    channelDropTarget = null;
  }

  /* Reorder by dropping on a channel of the same kind: the pointer's vertical
     half decides whether the dragged channel lands before or after it. */
  function handleChannelItemDragOver(
    event: DragEvent,
    ch: ChannelInfo | VoiceChannelInfo,
    voice: boolean
  ) {
    const dragged = draggedChannel;
    if (!dragged || dragged.voice !== voice || dragged.id === ch.id) return;
    event.preventDefault();
    // Keep the group-level handler from claiming the drag as a category move.
    event.stopPropagation();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    channelDropTarget = { id: ch.id, voice, after: event.clientY > rect.top + rect.height / 2 };
    dragOverKey = null;
  }

  function handleChannelItemDragLeave(ch: ChannelInfo | VoiceChannelInfo, voice: boolean) {
    if (channelDropTarget?.id === ch.id && channelDropTarget.voice === voice) {
      channelDropTarget = null;
    }
  }

  function handleChannelItemDrop(
    event: DragEvent,
    ch: ChannelInfo | VoiceChannelInfo,
    voice: boolean,
    group: CategoryGroup
  ) {
    const dragged = draggedChannel;
    const target = channelDropTarget;
    channelDropTarget = null;
    dragOverKey = null;
    if (!dragged || dragged.voice !== voice || dragged.id === ch.id) return;
    event.preventDefault();
    event.stopPropagation();
    const current = (voice ? group.voiceChannels : group.textChannels).map((c) => c.id);
    const order = current.filter((id) => id !== dragged.id);
    const after = target?.id === ch.id && target.voice === voice && target.after;
    order.splice(order.indexOf(ch.id) + (after ? 1 : 0), 0, dragged.id);
    const categoryId = group.category?.id ?? null;
    // Skip the round-trip when the drop changes nothing.
    if (
      categoryId === dragged.categoryId &&
      current.length === order.length &&
      current.every((id, index) => id === order[index])
    ) {
      draggedChannel = null;
      return;
    }
    channels.reorder(categoryId, order, voice);
    draggedChannel = null;
  }

  /* Category reordering via the headers. */
  function handleCategoryDragStart(event: DragEvent, category: CategoryInfo) {
    draggedCategoryId = category.id;
    if (!event.dataTransfer) return;
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData(CATEGORY_DRAG_MIME, String(category.id));
  }

  function handleCategoryDragEnd() {
    draggedCategoryId = null;
    categoryDropTarget = null;
  }

  function handleCategoryDragOver(event: DragEvent, category: CategoryInfo) {
    if (draggedCategoryId === null || draggedCategoryId === category.id) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    categoryDropTarget = { id: category.id, after: event.clientY > rect.top + rect.height / 2 };
  }

  function handleCategoryDragLeave(category: CategoryInfo) {
    if (categoryDropTarget?.id === category.id) categoryDropTarget = null;
  }

  function handleCategoryDrop(event: DragEvent, category: CategoryInfo) {
    const dragged = draggedCategoryId;
    const target = categoryDropTarget;
    draggedCategoryId = null;
    categoryDropTarget = null;
    if (dragged === null || dragged === category.id) return;
    event.preventDefault();
    event.stopPropagation();
    const order = sortedCategories.map((c) => c.id).filter((id) => id !== dragged);
    const after = target?.id === category.id && target.after;
    order.splice(order.indexOf(category.id) + (after ? 1 : 0), 0, dragged);
    categories.reorder(order);
  }

  /* Dragging a member out of one voice channel onto another asks the server
     to move them. It reaches the member as the same request a breakout room
     sends, and their client switches channel as if they had clicked. */
  let draggedMember: { user: string; channelId: number } | null = $state(null);
  let memberDropTarget: number | null = $state(null);

  function handleMemberDragStart(event: DragEvent, user: string, channelId: number) {
    draggedMember = { user, channelId };
    if (!event.dataTransfer) return;
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData(MEMBER_DRAG_MIME, user);
  }

  function handleMemberDragEnd() {
    draggedMember = null;
    memberDropTarget = null;
  }

  function handleMemberDragOver(event: DragEvent, channelId: number) {
    if (!draggedMember || draggedMember.channelId === channelId) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
    memberDropTarget = channelId;
  }

  function handleMemberDragLeave(event: DragEvent, channelId: number) {
    const related = event.relatedTarget;
    const current = event.currentTarget;
    if (related instanceof Node && current instanceof Node && current.contains(related)) return;
    if (memberDropTarget === channelId) memberDropTarget = null;
  }

  function handleMemberDrop(event: DragEvent, channelId: number) {
    const dragged = draggedMember;
    handleMemberDragEnd();
    if (!dragged || dragged.channelId === channelId) return;
    event.preventDefault();
    event.stopPropagation();
    chat.sendRaw({ type: 'move-member', user: dragged.user, channelId });
  }

  /** A channel can only be dropped on a category it is not already in. */
  function canDropOn(group: CategoryGroup, dragged: DraggedChannel | null): boolean {
    return dragged !== null && dragged.categoryId !== (group.category?.id ?? null);
  }

  function handleGroupDragOver(event: DragEvent, group: CategoryGroup) {
    if (!canDropOn(group, draggedChannel)) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
    dragOverKey = groupKey(group);
  }

  function handleGroupDragLeave(event: DragEvent, group: CategoryGroup) {
    // Ignore moves between descendants of the same group.
    const related = event.relatedTarget;
    const current = event.currentTarget;
    if (related instanceof Node && current instanceof Node && current.contains(related)) return;
    if (dragOverKey === groupKey(group)) dragOverKey = null;
  }

  function handleGroupDrop(event: DragEvent, group: CategoryGroup) {
    const dragged = draggedChannel;
    dragOverKey = null;
    if (!canDropOn(group, dragged) || !dragged) return;
    event.preventDefault();
    const categoryId = group.category?.id ?? null;
    channels.move(dragged.id, categoryId, dragged.voice);
    // Reveal the channel in its new home rather than dropping it out of sight.
    if (categoryId !== null && collapsedCategories.has(categoryId)) toggleCategory(categoryId);
    draggedChannel = null;
  }

  /** The empty "no category" group stays hidden unless it is a valid drop target. */
  function isGroupVisible(group: CategoryGroup, dragged: DraggedChannel | null): boolean {
    if (group.category) return true;
    if (group.textChannels.length || group.voiceChannels.length) return true;
    return canDropOn(group, dragged);
  }

  /** Custom sort order: position first, name and id as tie-breakers. */
  function byPosition(
    a: { position: number; name: string; id: number },
    b: { position: number; name: string; id: number }
  ): number {
    return a.position - b.position || a.name.localeCompare(b.name) || a.id - b.id;
  }

  let sortedCategories = $derived([...$categories].sort(byPosition));

  let categoryGroups = $derived((() => {
    const groups: CategoryGroup[] = [];
    const catMap = new Map<number, CategoryGroup>();

    for (const cat of sortedCategories) {
      const group: CategoryGroup = { category: cat, textChannels: [], voiceChannels: [] };
      catMap.set(cat.id, group);
      groups.push(group);
    }

    const uncategorized: CategoryGroup = { category: null, textChannels: [], voiceChannels: [] };

    for (const ch of $channels) {
      if (ch.categoryId != null && catMap.has(ch.categoryId)) {
        catMap.get(ch.categoryId)!.textChannels.push(ch);
      } else {
        uncategorized.textChannels.push(ch);
      }
    }

    for (const vc of $voiceChannels) {
      if (vc.categoryId != null && catMap.has(vc.categoryId)) {
        catMap.get(vc.categoryId)!.voiceChannels.push(vc);
      } else {
        uncategorized.voiceChannels.push(vc);
      }
    }

    // Always present so a channel can be dragged back out of a category; the
    // markup hides it while it is empty and no drag is in progress.
    groups.unshift(uncategorized);

    for (const group of groups) {
      group.textChannels.sort(byPosition);
      group.voiceChannels.sort(byPosition);
    }

    return groups;
  })());
</script>

<div class="channels" role="navigation" oncontextmenu={(e) => onOpenChannelMenu(e)} style="width: {$leftSidebarWidth}px">
  {#each categoryGroups as group (groupKey(group))}
    {#if isGroupVisible(group, draggedChannel)}
      <div
        class="category-group"
        class:drop-target={dragOverKey === groupKey(group)}
        role="group"
        aria-label={group.category?.name ?? t('channelSidebar.uncategorized')}
        ondragover={(e) => handleGroupDragOver(e, group)}
        ondragleave={(e) => handleGroupDragLeave(e, group)}
        ondrop={(e) => handleGroupDrop(e, group)}
      >
        {#if group.category}
          {@const category = group.category}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
          <h3
            class="section category-header"
            class:dragging={draggedCategoryId === category.id}
            class:drop-before={categoryDropTarget?.id === category.id && !categoryDropTarget.after}
            class:drop-after={categoryDropTarget?.id === category.id && categoryDropTarget.after}
            role="button"
            tabindex="0"
            draggable="true"
            ondragstart={(e) => handleCategoryDragStart(e, category)}
            ondragend={handleCategoryDragEnd}
            ondragover={(e) => handleCategoryDragOver(e, category)}
            ondragleave={() => handleCategoryDragLeave(category)}
            ondrop={(e) => handleCategoryDrop(e, category)}
            onclick={() => toggleCategory(category.id)}
            oncontextmenu={(e) => onOpenCategoryMenu(e, category)}
          >
            <span class="category-chevron" class:collapsed={collapsedCategories.has(category.id)}>&#9662;</span>
            {category.name}
          </h3>
        {:else}
          {#if group.textChannels.length}
            <h3 class="section">{t('channelSidebar.channels')}</h3>
          {/if}
        {/if}
        {#if !group.textChannels.length && !group.voiceChannels.length && !group.category}
          <p class="drop-hint">{t('channelSidebar.dropHereToRemove')}</p>
        {/if}
        {#if !group.category || !collapsedCategories.has(group.category.id)}
          {#each group.textChannels as ch (ch.id)}
            <button
              class:active={ch.id === currentChatChannelId}
              class:unread={ch.id !== currentChatChannelId && ($unread[ch.id]?.count ?? 0) > 0}
              class:dragging={draggedChannel?.id === ch.id && !draggedChannel.voice}
              class:drop-before={channelDropTarget?.id === ch.id && !channelDropTarget.voice && !channelDropTarget.after}
              class:drop-after={channelDropTarget?.id === ch.id && !channelDropTarget.voice && channelDropTarget.after}
              draggable="true"
              ondragstart={(e) => handleChannelDragStart(e, ch, false)}
              ondragend={handleChannelDragEnd}
              ondragover={(e) => handleChannelItemDragOver(e, ch, false)}
              ondragleave={() => handleChannelItemDragLeave(ch, false)}
              ondrop={(e) => handleChannelItemDrop(e, ch, false, group)}
              onclick={() => onJoinChannel(ch.id)}
              oncontextmenu={(e) => onOpenChannelMenu(e, ch.id)}
            >
              <span class="chan-icon">#</span>
              <span class="chan-name">{ch.name}</span>
              {#if ch.private}
                <span class="chan-lock" title={t('channelSidebar.privateChannel')} aria-label={t('channelSidebar.privateChannel')}>
                  <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>
                </span>
              {/if}
              {#if ch.e2ee}
                <span
                  class="chan-lock"
                  title={t('channelSidebar.endToEndEncrypted')}
                  aria-label={t('channelSidebar.endToEndEncrypted')}
                >
                  <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
                </span>
              {/if}
              {#if ch.id !== currentChatChannelId && ($unread[ch.id]?.count ?? 0) > 0}
                <span
                  class="unread-badge"
                  class:mention={($unread[ch.id]?.mentions ?? 0) > 0}
                  title={t('channelSidebar.unread', { count: $unread[ch.id].count })}
                >
                  {$unread[ch.id].count > 99 ? '99+' : $unread[ch.id].count}
                </span>
              {/if}
            </button>
          {/each}
          {#if group.voiceChannels.length}
            {#if !group.category && !group.textChannels.length}
              <h3 class="section">{t('channelSidebar.voiceChannels')}</h3>
            {/if}
          {/if}
          {#each orderVoiceChannels(group.voiceChannels) as row (row.channel.id)}
            {@const ch = row.channel}
            <div
              class="voice-group"
              class:breakout={row.room}
              class:member-drop-target={memberDropTarget === ch.id}
              role="group"
              aria-label={ch.name}
              ondragover={(e) => handleMemberDragOver(e, ch.id)}
              ondragleave={(e) => handleMemberDragLeave(e, ch.id)}
              ondrop={(e) => handleMemberDrop(e, ch.id)}
            >
              <button
                class:dragging={draggedChannel?.id === ch.id && draggedChannel.voice}
                class:drop-before={channelDropTarget?.id === ch.id && channelDropTarget.voice && !channelDropTarget.after}
                class:drop-after={channelDropTarget?.id === ch.id && channelDropTarget.voice && channelDropTarget.after}
                draggable={!row.room}
                ondragstart={(e) => handleChannelDragStart(e, ch, true)}
                ondragend={handleChannelDragEnd}
                ondragover={(e) => handleChannelItemDragOver(e, ch, true)}
                ondragleave={() => handleChannelItemDragLeave(ch, true)}
                ondrop={(e) => handleChannelItemDrop(e, ch, true, group)}
                onclick={() => onJoinVoiceChannel(ch.id)}
                oncontextmenu={(e) => onOpenChannelMenu(e, ch.id, true)}
              >
                <span class="chan-icon">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M15.54 8.46a5 5 0 0 1 0 7.07"/><path d="M19.07 4.93a10 10 0 0 1 0 14.14"/></svg>
                </span>
                <span class="voice-channel-name">{ch.name}</span>
                {#if row.room}
                  <span class="badge breakout-badge" title={t('channelSidebar.temporaryBreakoutRoom')}>{t('channelSidebar.breakout')}</span>
                {/if}
                {#if ch.private}
                  <span class="chan-lock" title={t('channelSidebar.privateChannel')} aria-label={t('channelSidebar.privateChannel')}>
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>
                  </span>
                {/if}
                <span class="voice-channel-quality">{formatVoiceQuality(ch)}</span>
                {#if ch.userLimit > 0}
                  <span class="voice-channel-limit" title={t('channelSidebar.userLimit')}>
                    {$voiceUsers[ch.id]?.length ?? 0}/{ch.userLimit}
                  </span>
                {/if}
              </button>
              {#if $voiceUsers[ch.id]?.length}
                {@const queue = handQueue($voiceHands, ch.id)}
                <ul class="voice-user-list">
                  {#each $voiceUsers[ch.id] as user}
                    {@const mute =
                      user === $session.user
                        ? { micMuted: $microphoneMuted, outputMuted: $outputMuted }
                        : ($voiceMuteStates[user] ?? { micMuted: false, outputMuted: false })}
                    {@const talking = Boolean($speakingUsers[user]) && !mute.micMuted}
                    {@const movable = canMoveMembers && user !== $session.user}
                    <li
                      draggable={movable}
                      class:dragging={draggedMember?.user === user}
                      ondragstart={(e) => movable && handleMemberDragStart(e, user, ch.id)}
                      ondragend={handleMemberDragEnd}
                      oncontextmenu={(e) => user !== $session.user && onOpenUserVolumeMenu(e, user)}
                      class:clickable={user !== $session.user}
                      class:talking
                    >
                      <span class="status voice" class:talking></span>
                      <span
                        class="username"
                        style={$roles[user]?.color ? `color: ${$roles[user].color}` : ''}
                        >{$displayNames(user)}</span
                      >
                      {#if $roles[user]?.icon}
                        <RoleIcon icon={$roles[user].icon} role={$roles[user].iconRole} />
                      {/if}
                      {#if $roles[user]}
                        <span
                          class="role"
                          style={$roles[user].color ? `color: ${$roles[user].color}` : ''}
                          >{$roles[user].role}</span
                        >
                      {/if}
                      {#if mute.micMuted || mute.outputMuted}
                        <span class="mute-icons">
                          {#if mute.micMuted}
                            <span class="mute-icon" title={t('channelSidebar.microphoneMuted')} aria-label={t('channelSidebar.microphoneMuted')}>
                              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="2" y1="2" x2="22" y2="22"/><path d="M18.89 13.23A7.12 7.12 0 0 0 19 12v-2"/><path d="M5 10v2a7 7 0 0 0 12 5"/><path d="M15 9.34V5a3 3 0 0 0-5.68-1.33"/><path d="M9 9v3a3 3 0 0 0 5.12 2.12"/><line x1="12" y1="19" x2="12" y2="22"/></svg>
                            </span>
                          {/if}
                          {#if mute.outputMuted}
                            <span class="mute-icon" title={t('channelSidebar.speakerMuted')} aria-label={t('channelSidebar.speakerMuted')}>
                              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><line x1="22" y1="9" x2="16" y2="15"/><line x1="16" y1="9" x2="22" y2="15"/></svg>
                            </span>
                          {/if}
                        </span>
                      {/if}
                      {#if queue.includes(user)}
                        {@const place = queue.indexOf(user) + 1}
                        <span
                          class="hand-raised"
                          title={t('channelSidebar.handRaisedTooltip', { place })}
                          aria-label={t('channelSidebar.handRaisedLabel', { place })}
                        >
                          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 11V6a2 2 0 0 0-4 0"/><path d="M14 10V4a2 2 0 0 0-4 0v2"/><path d="M10 10.5V6a2 2 0 0 0-4 0v8"/><path d="M18 8a2 2 0 1 1 4 0v6a8 8 0 0 1-8 8h-2c-2.8 0-4.5-.86-5.99-2.34l-3.6-3.6a2 2 0 0 1 2.83-2.82L7 15"/></svg>
                          <span>{place}</span>
                        </span>
                      {/if}
                      {#if $activeScreenShares[ch.id]?.includes(user)}
                        {@const isOwnShare = user === $session.user}
                        {@const watching = $watchedScreenShares.includes(user)}
                        {@const selfLabel = $screenSharePreview
                          ? t('channelSidebar.hidePreview')
                          : t('channelSidebar.showPreview')}
                        {@const peerLabel = watching
                          ? t('channelSidebar.stopWatching', { name: $displayNames(user) })
                          : t('channelSidebar.watch', { name: $displayNames(user) })}
                        <button
                          class="screenshare-indicator"
                          class:muted={isOwnShare && !$screenSharePreview}
                          class:watching={!isOwnShare && watching}
                          onclick={() => onViewScreenShare(user)}
                          title={isOwnShare ? selfLabel : peerLabel}
                          aria-label={isOwnShare ? selfLabel : peerLabel}
                          aria-pressed={isOwnShare ? $screenSharePreview : watching}
                        >
                          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="2" y="3" width="20" height="14" rx="2" ry="2"/><line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="17" x2="12" y2="21"/></svg>
                          <span>{t('channelSidebar.live')}</span>
                        </button>
                      {/if}
                      {#if $activeWebcams[ch.id]?.includes(user)}
                        <span
                          class="webcam-indicator"
                          title={t('channelSidebar.cameraOn', { name: $displayNames(user) })}
                          aria-label={t('channelSidebar.cameraOn', { name: $displayNames(user) })}
                        >
                          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polygon points="23 7 16 12 23 17 23 7"/><rect x="1" y="5" width="15" height="14" rx="2" ry="2"/></svg>
                        </span>
                      {/if}
                      <ConnectionBars
                        strength={user === $session.user ? serverStrength : ($voiceStats[user]?.strength ?? 0)}
                        reconnecting={user !== $session.user && $voiceReconnecting.has(user)}
                      />
                    </li>
                  {/each}
                </ul>
              {/if}
            </div>
          {/each}
        {/if}
      </div>
    {/if}
  {/each}

  <div class="voice-controls-container">
    <div class="voice-controls-panel">
      {#if inVoice}
        <div class="voice-controls-header">
          {t('channelSidebar.voiceControls')}
          {#if $viaServer}
            <span
              class="badge via-server"
              title={t('channelSidebar.thisChannelIsLarge')}
            >
              {t('channelSidebar.viaServer')}
            </span>
          {/if}
        </div>
      {/if}
      <div class="voice-controls-buttons">
        {#if inVoice}
          <button class="voice-control-btn leave" onclick={onLeaveVoice}>
            <span class="btn-icon">
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" y1="12" x2="9" y2="12"/></svg>
            </span>
            <span class="btn-text">{t('channelSidebar.leaveVoice')}</span>
          </button>
        {/if}
        <button
          class="voice-control-btn mute"
          class:muted={inVoice && $microphoneMuted}
          class:active={inVoice &&
            $voiceMode !== 'ptt' &&
            Boolean($session.user && $speakingUsers[$session.user])}
          class:ptt-active={inVoice && $voiceMode === 'ptt' && $isPttActive}
          class:disabled={!inVoice || !$canSpeak}
          onclick={onToggleMicrophone}
          disabled={!inVoice || !$canSpeak}
          title={t(
            !inVoice
              ? 'channelSidebar.joinVoiceFirst'
              : !$canSpeak
                ? 'channelSidebar.noTalkPermission'
                : $microphoneMuted
                  ? 'channelSidebar.unmuteMicrophone'
                  : 'channelSidebar.muteMicrophone'
          )}
        >
          <span class="btn-icon">
            {#if inVoice && $microphoneMuted}
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="2" y1="2" x2="22" y2="22"/><path d="M18.89 13.23A7.12 7.12 0 0 0 19 12v-2"/><path d="M5 10v2a7 7 0 0 0 12 5"/><path d="M15 9.34V5a3 3 0 0 0-5.68-1.33"/><path d="M9 9v3a3 3 0 0 0 5.12 2.12"/><line x1="12" y1="19" x2="12" y2="22"/></svg>
            {:else}
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" y1="19" x2="12" y2="22"/></svg>
            {/if}
          </span>
          <span class="btn-text">
            {#if !inVoice}
              {t('channelSidebar.mic.idle')}
            {:else if !$canSpeak}
              {t('channelSidebar.mic.listenOnly')}
            {:else if $microphoneMuted}
              {t('channelSidebar.mic.unmute')}
            {:else if $voiceMode === 'continuous'}
              {t('channelSidebar.mic.continuous')}
            {:else if $voiceMode === 'vad'}
              {t('channelSidebar.mic.vad')}
            {:else if $voiceMode === 'ptt'}
              {t('channelSidebar.mic.ptt')}
            {:else}
              {t('channelSidebar.mic.mute')}
            {/if}
          </span>
        </button>
        <button
          class="voice-control-btn mute"
          class:muted={inVoice && $outputMuted}
          class:disabled={!inVoice}
          onclick={onToggleOutput}
          disabled={!inVoice}
          title={t(
            !inVoice
              ? 'channelSidebar.joinVoiceFirst'
              : $outputMuted
                ? 'channelSidebar.unmuteOutput'
                : 'channelSidebar.muteOutput'
          )}
        >
          <span class="btn-icon">
            {#if inVoice && $outputMuted}
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><line x1="22" y1="9" x2="16" y2="15"/><line x1="16" y1="9" x2="22" y2="15"/></svg>
            {:else}
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M15.54 8.46a5 5 0 0 1 0 7.07"/><path d="M19.07 4.93a10 10 0 0 1 0 14.14"/></svg>
            {/if}
          </span>
          <span class="btn-text">{t(!inVoice ? 'channelSidebar.speaker' : $outputMuted ? 'channelSidebar.unmuteOut' : 'channelSidebar.muteOut')}</span>
        </button>
        {#if inVoice && currentVoiceChannelId !== null}
          {@const channelId = currentVoiceChannelId}
          <button
            class="voice-control-btn hand"
            class:raised={handRaised}
            onclick={() => sendHand(channelId, !handRaised)}
            aria-pressed={handRaised}
            title={t(handRaised ? 'channelSidebar.lowerHandTooltip' : 'channelSidebar.raiseHandTooltip')}
          >
            <span class="btn-icon"><svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 11V6a2 2 0 0 0-4 0"/><path d="M14 10V4a2 2 0 0 0-4 0v2"/><path d="M10 10.5V6a2 2 0 0 0-4 0v8"/><path d="M18 8a2 2 0 1 1 4 0v6a8 8 0 0 1-8 8h-2c-2.8 0-4.5-.86-5.99-2.34l-3.6-3.6a2 2 0 0 1 2.83-2.82L7 15"/></svg></span>
            <span class="btn-text">{t(handRaised ? 'channelSidebar.lowerHand' : 'channelSidebar.raiseHand')}</span>
          </button>
        {/if}
      </div>

      {#if inVoice}
        <ScreenShareControls currentVoiceChannel={currentVoiceChannelId} {inVoice} />
        <WebcamControls currentVoiceChannel={currentVoiceChannelId} {inVoice} />
        <SoundboardPanel
          currentVoiceChannel={currentVoiceChannelId}
          {inVoice}
          canPlay={canUseSoundboard}
          canManage={canManageSounds}
        />
      {/if}
    </div>
  </div>
</div>

<style>
  /* Left pane: sits on the app background, no card chrome. */
  .channels {
    width: clamp(200px, 18vw, 280px);
    flex-shrink: 0;
    background: var(--color-bg);
    padding: var(--space-3) var(--space-2);
    display: flex;
    flex-direction: column;
    gap: 1px;
    overflow-y: auto;
  }

  .channels .section {
    margin: var(--space-3) var(--space-2) var(--space-1);
    font-size: var(--text-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-muted);
  }

  /* Scoped to the first group so every later group keeps its top spacing. */
  .channels > .category-group:first-child .section {
    margin-top: 0;
  }

  .category-group {
    display: flex;
    flex-direction: column;
    gap: 1px;
    border-radius: var(--radius-sm);
    border: 1px dashed transparent;
  }

  .voice-group.member-drop-target {
    background: color-mix(in srgb, var(--color-primary) 8%, transparent);
    outline: 1px dashed var(--color-primary);
    border-radius: var(--radius-sm);
  }

  .category-group.drop-target {
    background: color-mix(in srgb, var(--color-primary) 8%, transparent);
    border-color: var(--color-primary);
  }

  .drop-hint {
    margin: 0;
    padding: var(--space-2);
    text-align: center;
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .channels button.dragging,
  .category-header.dragging,
  .voice-user-list li.dragging {
    opacity: 0.5;
  }

  /* Insertion line for reordering: marks the edge the drop will land on. */
  .channels button.drop-before,
  .category-header.drop-before {
    box-shadow: 0 -2px 0 0 var(--color-primary);
  }

  .channels button.drop-after,
  .category-header.drop-after {
    box-shadow: 0 2px 0 0 var(--color-primary);
  }

  .category-header {
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: var(--space-1);
    user-select: none;
    border-radius: var(--radius-xs);
    padding: var(--space-1) var(--space-2);
    margin-inline: 0;
    transition: color var(--transition);
  }

  .category-header:hover {
    color: var(--color-on-surface-variant);
  }

  .category-chevron {
    display: inline-block;
    font-size: 0.625rem;
    transition: transform var(--motion-duration-medium) var(--motion-easing-standard);
    flex-shrink: 0;
  }

  .category-chevron.collapsed {
    transform: rotate(-90deg);
  }

  .channels button {
    width: 100%;
    min-height: 2rem;
    padding: var(--space-1) var(--space-2);
    border: none;
    background: transparent;
    color: var(--color-muted);
    text-align: left;
    border-radius: var(--radius-sm);
    font-weight: 500;
    font-size: var(--text-md);
    display: flex;
    align-items: center;
    gap: var(--space-2);
    position: relative;
  }

  .channels button:hover {
    background: var(--color-surface-elevated);
    color: var(--color-on-surface-variant);
  }

  .channels button.active {
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
  }

  .chan-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1rem;
    opacity: 0.7;
    flex-shrink: 0;
    font-weight: 500;
  }

  .chan-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chan-lock {
    display: inline-flex;
    align-items: center;
    color: var(--color-muted);
    flex-shrink: 0;
  }

  .channels button.unread {
    color: var(--color-on-surface);
    font-weight: 600;
  }

  .unread-badge {
    flex-shrink: 0;
    min-width: 1.25rem;
    padding: 0 var(--space-1);
    border-radius: var(--radius-pill);
    text-align: center;
    font-size: var(--text-xs);
    font-weight: 600;
    line-height: 1.125rem;
    background: var(--color-surface-raised);
    color: var(--color-on-surface-variant);
  }

  .unread-badge.mention {
    background: var(--color-error);
    color: var(--color-surface);
  }

  .voice-group {
    display: flex;
    flex-direction: column;
  }

  /* A breakout room sits under the call it was split off from; the indent is
     what says "this is part of that" without a second list. */
  .voice-group.breakout {
    padding-left: var(--space-4);
  }

  .breakout-badge {
    flex-shrink: 0;
  }

  .voice-user-list {
    list-style: none;
    margin: 0;
    padding: 0 0 var(--space-1) var(--space-5);
    display: flex;
    flex-direction: column;
  }

  .voice-user-list li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-xs);
    font-size: var(--text-sm);
    transition: background var(--transition);
  }

  .voice-user-list li.clickable {
    cursor: context-menu;
  }

  .voice-user-list li:hover {
    background: var(--color-surface-elevated);
  }

  .voice-user-list li.talking {
    background: color-mix(in srgb, var(--color-success) 10%, transparent);
  }

  .voice-user-list .status.voice {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: var(--color-success);
    opacity: 0.4;
    flex-shrink: 0;
    transition: opacity 0.15s ease-in, box-shadow 0.15s ease-in;
  }

  .voice-user-list .status.voice.talking {
    opacity: 1;
    box-shadow: 0 0 4px color-mix(in srgb, var(--color-success) 60%, transparent);
    transition: opacity 0.05s ease-out, box-shadow 0.05s ease-out;
  }

  /* The name is the identity and the role badge decoration, so in the
     narrow default sidebar the badge truncates first: with a zero flex basis
     the name used to be the first thing squeezed out of a busy row. */
  .voice-user-list .username {
    font-weight: 500;
    color: var(--color-on-surface-variant);
    flex: 1 0.2 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .voice-user-list .role {
    font-size: var(--text-xs);
    font-weight: 500;
    opacity: 0.75;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mute-icons {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    flex-shrink: 0;
    color: var(--color-error);
  }

  .mute-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .hand-raised {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
    color: var(--color-warning);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  /* Name and quality both shrink from their content width, so at the
     default sidebar width the name keeps a share instead of collapsing to
     nothing behind a fixed "Standard (64 kbps)". */
  .voice-channel-name {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .voice-channel-quality {
    margin-left: auto;
    font-size: var(--text-xs);
    font-weight: 500;
    color: var(--color-muted);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .voice-channel-limit {
    font-size: var(--text-xs);
    font-family: var(--font-mono);
    color: var(--color-muted);
    flex-shrink: 0;
  }

  /* Voice controls dock at the bottom of the pane. */
  .voice-controls-container {
    margin-top: auto;
    padding-top: var(--space-3);
  }

  .voice-controls-panel {
    border-radius: var(--radius-md);
    padding: var(--space-2);
    background: var(--color-surface-elevated);
    border: 1px solid var(--color-surface-outline);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .voice-controls-header {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--color-muted);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    padding: var(--space-1) var(--space-1) 0;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: var(--space-1) var(--space-2);
  }

  .via-server {
    white-space: nowrap;
    text-transform: none;
    letter-spacing: normal;
    cursor: help;
  }

  .voice-controls-buttons {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .voice-control-btn {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--control-height);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    border: 1px solid transparent;
    background: transparent;
    color: var(--color-on-surface-variant);
    font-weight: 500;
    font-size: var(--text-sm);
    width: 100%;
  }

  .voice-control-btn:hover:not(:disabled) {
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
  }

  .btn-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.25rem;
    flex-shrink: 0;
  }

  .btn-text {
    font-size: var(--text-sm);
  }

  .voice-control-btn.leave {
    color: var(--color-error);
  }

  .voice-control-btn.leave:hover {
    background: color-mix(in srgb, var(--color-error) 12%, transparent);
    color: var(--color-error);
  }

  .voice-control-btn.hand.raised {
    background: color-mix(in srgb, var(--color-warning) 14%, transparent);
    color: var(--color-warning);
  }

  .voice-control-btn.disabled {
    opacity: 0.45;
    cursor: default;
    pointer-events: none;
  }

  .voice-control-btn.mute.muted {
    background: color-mix(in srgb, var(--color-error) 12%, transparent);
    color: var(--color-error);
  }

  .voice-control-btn.mute.active,
  .voice-control-btn.mute.ptt-active {
    background: color-mix(in srgb, var(--color-success) 12%, transparent);
    color: var(--color-success);
  }

  /* Clickable "LIVE" pill on users who are sharing their screen, shaped
     like the unread badge. The `.channels button.` prefix is required to
     out-rank the generic full-width `.channels button` sizing above. */
  .channels button.screenshare-indicator {
    width: auto;
    min-height: 0;
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    padding: 0 var(--space-2);
    border-radius: var(--radius-pill);
    background: var(--color-primary-container);
    color: var(--color-primary);
    font-size: var(--text-xs);
    font-weight: 600;
    line-height: 1.125rem;
    letter-spacing: 0.04em;
  }

  .channels button.screenshare-indicator:hover {
    background: color-mix(in srgb, var(--color-primary) 24%, transparent);
    color: var(--color-primary);
  }

  /* A share we currently have open on the stage — clicking it again stops
     watching that one and leaves the other shares alone. */
  .channels button.screenshare-indicator.watching {
    background: var(--color-primary);
    color: var(--color-on-primary);
  }

  .channels button.screenshare-indicator.watching:hover {
    background: var(--color-primary);
    color: var(--color-on-primary);
  }

  /* Our own share with the self-preview switched off: still live, just not
     being rendered back to us. */
  .channels button.screenshare-indicator.muted {
    background: var(--color-surface-raised);
    color: var(--color-muted);
  }

  .screenshare-indicator svg {
    flex-shrink: 0;
  }

  /* A camera that is on. Not a button: unlike a screen share there is nothing
     to open — every camera in the channel is already on the video stage. */
  .webcam-indicator {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    color: var(--color-primary);
  }
</style>
