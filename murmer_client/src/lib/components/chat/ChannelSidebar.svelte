<!--
  Left-hand sidebar: text/voice channel list grouped by category, the users in
  each voice channel and the voice control panel (mute, leave, screen share).
-->
<script lang="ts">
  import { browser } from '$app/environment';
  import ConnectionBars from '$lib/components/ConnectionBars.svelte';
  import ScreenShareControls from '$lib/components/ScreenShareControls.svelte';
  import { channels } from '$lib/stores/channels';
  import { voiceChannels } from '$lib/stores/voiceChannels';
  import { categories } from '$lib/stores/categories';
  import { voiceUsers } from '$lib/stores/voiceUsers';
  import { voiceStats } from '$lib/stores/voice';
  import { session } from '$lib/stores/session';
  import { roles } from '$lib/stores/roles';
  import { leftSidebarWidth } from '$lib/stores/layout';
  import { microphoneMuted, outputMuted, voiceMode, voiceActivity, isPttActive } from '$lib/stores/settings';
  import { remoteSpeaking } from '$lib/stores/voiceSpeaking';
  import { voiceMuteStates } from '$lib/stores/voiceMute';
  import { activeScreenShares } from '$lib/stores/screenShare';
  import { unread } from '$lib/stores/unread';
  import { formatVoiceQuality } from '$lib/chat/helpers';
  import type { CategoryInfo, ChannelInfo, VoiceChannelInfo } from '$lib/types';

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

  const COLLAPSED_KEY = 'murmer_collapsed_categories';
  const UNCATEGORIZED_KEY = '__uncategorized';
  /* Custom MIME type so the chat page's file-drop zone (which only reacts to
     dragged files) never mistakes a channel drag for an upload. */
  const CHANNEL_DRAG_MIME = 'application/x-murmer-channel';

  function loadCollapsed(): Set<number> {
    if (!browser) return new Set();
    try {
      const parsed = JSON.parse(localStorage.getItem(COLLAPSED_KEY) ?? '[]');
      return new Set(Array.isArray(parsed) ? parsed.filter((v) => typeof v === 'number') : []);
    } catch {
      return new Set();
    }
  }

  let collapsedCategories: Set<number> = $state(loadCollapsed());
  function toggleCategory(id: number) {
    if (collapsedCategories.has(id)) {
      collapsedCategories.delete(id);
    } else {
      collapsedCategories.add(id);
    }
    collapsedCategories = collapsedCategories;
    if (browser) {
      localStorage.setItem(COLLAPSED_KEY, JSON.stringify([...collapsedCategories]));
    }
  }

  /* Channel drag & drop. The dragged channel is kept in component state (a drag
     never leaves this component) while the DataTransfer payload only exists so
     the browser reports a valid drag type during `dragover`. Permission to move
     a channel is enforced by the server, mirroring the "Move to" context menu. */
  let draggedChannel: DraggedChannel | null = $state(null);
  let dragOverKey: string | null = $state(null);

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

  let categoryGroups = $derived((() => {
    const groups: CategoryGroup[] = [];
    const catMap = new Map<number, CategoryGroup>();

    for (const cat of $categories) {
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
        aria-label={group.category?.name ?? 'Uncategorized channels'}
        ondragover={(e) => handleGroupDragOver(e, group)}
        ondragleave={(e) => handleGroupDragLeave(e, group)}
        ondrop={(e) => handleGroupDrop(e, group)}
      >
        {#if group.category}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
          <h3
            class="section category-header"
            role="button"
            tabindex="0"
            onclick={() => toggleCategory(group.category?.id ?? 0)}
            oncontextmenu={(e) => { if (group.category) onOpenCategoryMenu(e, group.category); }}
          >
            <span class="category-chevron" class:collapsed={collapsedCategories.has(group.category?.id ?? 0)}>&#9662;</span>
            {group.category?.name ?? ''}
          </h3>
        {:else}
          {#if group.textChannels.length}
            <h3 class="section">Channels</h3>
          {/if}
        {/if}
        {#if !group.textChannels.length && !group.voiceChannels.length && !group.category}
          <p class="drop-hint">Drop here to remove from category</p>
        {/if}
        {#if !group.category || !collapsedCategories.has(group.category.id)}
          {#each group.textChannels as ch (ch.id)}
            <button
              class:active={ch.id === currentChatChannelId}
              class:unread={ch.id !== currentChatChannelId && ($unread[ch.id]?.count ?? 0) > 0}
              class:dragging={draggedChannel?.id === ch.id && !draggedChannel.voice}
              draggable="true"
              ondragstart={(e) => handleChannelDragStart(e, ch, false)}
              ondragend={handleChannelDragEnd}
              onclick={() => onJoinChannel(ch.id)}
              oncontextmenu={(e) => onOpenChannelMenu(e, ch.id)}
            >
              <span class="chan-icon">#</span>
              <span class="chan-name">{ch.name}</span>
              {#if ch.id !== currentChatChannelId && ($unread[ch.id]?.count ?? 0) > 0}
                <span
                  class="unread-badge"
                  class:mention={($unread[ch.id]?.mentions ?? 0) > 0}
                  title={`${$unread[ch.id].count} unread message${$unread[ch.id].count === 1 ? '' : 's'}`}
                >
                  {$unread[ch.id].count > 99 ? '99+' : $unread[ch.id].count}
                </span>
              {/if}
            </button>
          {/each}
          {#if group.voiceChannels.length}
            {#if !group.category && !group.textChannels.length}
              <h3 class="section">Voice Channels</h3>
            {/if}
          {/if}
          {#each group.voiceChannels as ch (ch.id)}
            <div class="voice-group">
              <button
                class:dragging={draggedChannel?.id === ch.id && draggedChannel.voice}
                draggable="true"
                ondragstart={(e) => handleChannelDragStart(e, ch, true)}
                ondragend={handleChannelDragEnd}
                onclick={() => onJoinVoiceChannel(ch.id)}
                oncontextmenu={(e) => onOpenChannelMenu(e, ch.id, true)}
              >
                <span class="chan-icon">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M15.54 8.46a5 5 0 0 1 0 7.07"/><path d="M19.07 4.93a10 10 0 0 1 0 14.14"/></svg>
                </span>
                <span class="voice-channel-name">{ch.name}</span>
                <span class="voice-channel-quality">{formatVoiceQuality(ch)}</span>
              </button>
              {#if $voiceUsers[ch.id]?.length}
                <ul class="voice-user-list">
                  {#each $voiceUsers[ch.id] as user}
                    {@const mute =
                      user === $session.user
                        ? { micMuted: $microphoneMuted, outputMuted: $outputMuted }
                        : ($voiceMuteStates[user] ?? { micMuted: false, outputMuted: false })}
                    <li
                      oncontextmenu={(e) => user !== $session.user && onOpenUserVolumeMenu(e, user)}
                      class:clickable={user !== $session.user}
                      class:talking={
                        user === $session.user
                          ? !$microphoneMuted && $voiceActivity
                          : Boolean($remoteSpeaking[user])
                      }
                    >
                      <span
                        class="status voice"
                        class:talking={
                          user === $session.user
                            ? !$microphoneMuted && $voiceActivity
                            : Boolean($remoteSpeaking[user])
                        }
                      ></span>
                      <span
                        class="username"
                        style={$roles[user]?.color ? `color: ${$roles[user].color}` : ''}
                        >{user}</span
                      >
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
                            <span class="mute-icon" title="Microphone muted" aria-label="Microphone muted">
                              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="2" y1="2" x2="22" y2="22"/><path d="M18.89 13.23A7.12 7.12 0 0 0 19 12v-2"/><path d="M5 10v2a7 7 0 0 0 12 5"/><path d="M15 9.34V5a3 3 0 0 0-5.68-1.33"/><path d="M9 9v3a3 3 0 0 0 5.12 2.12"/><line x1="12" y1="19" x2="12" y2="22"/></svg>
                            </span>
                          {/if}
                          {#if mute.outputMuted}
                            <span class="mute-icon" title="Speaker muted" aria-label="Speaker muted">
                              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><line x1="22" y1="9" x2="16" y2="15"/><line x1="16" y1="9" x2="22" y2="15"/></svg>
                            </span>
                          {/if}
                        </span>
                      {/if}
                      {#if $activeScreenShares[ch.id]?.includes(user) && user !== $session.user}
                        <button
                          class="screenshare-indicator"
                          onclick={() => onViewScreenShare(user)}
                          title="View {user}'s screen"
                          aria-label="View {user}'s screen share"
                        >
                          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M9 17.25v1.007a3 3 0 01-.879 2.122L7.5 21h9l-.621-.621A3 3 0 0115 18.257V17.25m6-12V15a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 15V5.25m18 0A2.25 2.25 0 0018.75 3H5.25A2.25 2.25 0 003 5.25m18 0V12a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 12V5.25" />
                          </svg>
                        </button>
                      {/if}
                      <ConnectionBars
                        strength={user === $session.user ? serverStrength : ($voiceStats[user]?.strength ?? 0)}
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
        <div class="voice-controls-header">Voice Controls</div>
      {/if}
      <div class="voice-controls-buttons">
        {#if inVoice}
          <button class="voice-control-btn leave" onclick={onLeaveVoice}>
            <span class="btn-icon">
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" y1="12" x2="9" y2="12"/></svg>
            </span>
            <span class="btn-text">Leave Voice</span>
          </button>
        {/if}
        <button
          class="voice-control-btn mute"
          class:muted={inVoice && $microphoneMuted}
          class:active={inVoice && $voiceMode === 'vad' && $voiceActivity}
          class:ptt-active={inVoice && $voiceMode === 'ptt' && $isPttActive}
          class:disabled={!inVoice}
          onclick={onToggleMicrophone}
          disabled={!inVoice}
          title={!inVoice ? 'Join a voice channel first' : $microphoneMuted ? 'Unmute Microphone' : 'Mute Microphone'}
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
              Microphone
            {:else if $microphoneMuted}
              Unmute Mic
            {:else if $voiceMode === 'continuous'}
              Always On
            {:else if $voiceMode === 'vad'}
              Voice Detection
            {:else if $voiceMode === 'ptt'}
              Push to Talk
            {:else}
              Mute Mic
            {/if}
          </span>
        </button>
        <button
          class="voice-control-btn mute"
          class:muted={inVoice && $outputMuted}
          class:disabled={!inVoice}
          onclick={onToggleOutput}
          disabled={!inVoice}
          title={!inVoice ? 'Join a voice channel first' : $outputMuted ? 'Unmute Output' : 'Mute Output'}
        >
          <span class="btn-icon">
            {#if inVoice && $outputMuted}
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><line x1="22" y1="9" x2="16" y2="15"/><line x1="16" y1="9" x2="22" y2="15"/></svg>
            {:else}
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M15.54 8.46a5 5 0 0 1 0 7.07"/><path d="M19.07 4.93a10 10 0 0 1 0 14.14"/></svg>
            {/if}
          </span>
          <span class="btn-text">{!inVoice ? 'Speaker' : $outputMuted ? 'Unmute Out' : 'Mute Out'}</span>
        </button>
      </div>

      {#if inVoice}
        <ScreenShareControls currentVoiceChannel={currentVoiceChannelId} {inVoice} />
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

  .channels button.dragging {
    opacity: 0.5;
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

  .voice-user-list .username {
    font-weight: 500;
    color: var(--color-on-surface-variant);
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .voice-user-list .role {
    font-size: var(--text-xs);
    font-weight: 500;
    opacity: 0.75;
    flex-shrink: 0;
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

  .voice-channel-name {
    flex: 1;
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

  .screenshare-indicator {
    background: transparent;
    border: none;
    padding: var(--space-1);
    cursor: pointer;
    color: var(--color-primary);
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-xs);
    flex-shrink: 0;
    width: auto;
    min-height: 0;
  }

  .screenshare-indicator:hover {
    background: var(--color-primary-container);
    color: var(--color-primary);
  }

  .screenshare-indicator svg {
    width: 0.875rem;
    height: 0.875rem;
  }
</style>
