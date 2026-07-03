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
  import { activeScreenShares } from '$lib/stores/screenShare';
  import { formatVoiceQuality } from '$lib/chat/helpers';
  import type { CategoryInfo, ChannelInfo, VoiceChannelInfo } from '$lib/types';

  export let currentChatChannelId: number;
  export let currentVoiceChannelId: number | null;
  export let inVoice: boolean;
  export let serverStrength: number;
  export let onJoinChannel: (id: number) => void;
  export let onJoinVoiceChannel: (id: number) => void;
  export let onOpenChannelMenu: (event: MouseEvent, channelId?: number, voice?: boolean) => void;
  export let onOpenCategoryMenu: (event: MouseEvent, category: CategoryInfo) => void;
  export let onOpenUserVolumeMenu: (event: MouseEvent, user: string) => void;
  export let onViewScreenShare: (user: string) => void;
  export let onLeaveVoice: () => void;
  export let onToggleMicrophone: () => void;
  export let onToggleOutput: () => void;

  interface CategoryGroup {
    category: CategoryInfo | null;
    textChannels: ChannelInfo[];
    voiceChannels: VoiceChannelInfo[];
  }

  const COLLAPSED_KEY = 'murmer_collapsed_categories';

  function loadCollapsed(): Set<number> {
    if (!browser) return new Set();
    try {
      const parsed = JSON.parse(localStorage.getItem(COLLAPSED_KEY) ?? '[]');
      return new Set(Array.isArray(parsed) ? parsed.filter((v) => typeof v === 'number') : []);
    } catch {
      return new Set();
    }
  }

  let collapsedCategories: Set<number> = loadCollapsed();
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

  $: categoryGroups = (() => {
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

    if (uncategorized.textChannels.length || uncategorized.voiceChannels.length) {
      groups.unshift(uncategorized);
    }

    return groups;
  })();
</script>

<div class="channels" role="navigation" on:contextmenu={(e) => onOpenChannelMenu(e)} style="width: {$leftSidebarWidth}px">
  {#each categoryGroups as group (group.category?.id ?? '__uncategorized')}
    {#if group.category}
      <!-- svelte-ignore a11y-click-events-have-key-events -->
      <!-- svelte-ignore a11y-no-noninteractive-element-to-interactive-role -->
      <h3
        class="section category-header"
        role="button"
        tabindex="0"
        on:click={() => toggleCategory(group.category?.id ?? 0)}
        on:contextmenu={(e) => { if (group.category) onOpenCategoryMenu(e, group.category); }}
      >
        <span class="category-chevron" class:collapsed={collapsedCategories.has(group.category?.id ?? 0)}>&#9662;</span>
        {group.category?.name ?? ''}
      </h3>
    {:else}
      {#if group.textChannels.length}
        <h3 class="section">Channels</h3>
      {/if}
    {/if}
    {#if !group.category || !collapsedCategories.has(group.category.id)}
      {#each group.textChannels as ch (ch.id)}
        <button
          class:active={ch.id === currentChatChannelId}
          on:click={() => onJoinChannel(ch.id)}
          on:contextmenu={(e) => onOpenChannelMenu(e, ch.id)}
        >
          <span class="chan-icon">#</span> {ch.name}
        </button>
      {/each}
      {#if group.voiceChannels.length}
        {#if !group.category && !group.textChannels.length}
          <h3 class="section">Voice Channels</h3>
        {/if}
      {/if}
      {#each group.voiceChannels as ch (ch.id)}
        <div class="voice-group">
          <button on:click={() => onJoinVoiceChannel(ch.id)} on:contextmenu={(e) => onOpenChannelMenu(e, ch.id, true)}>
            <span class="chan-icon">&#x1f50a;</span>
            <span class="voice-channel-name">{ch.name}</span>
            <span class="voice-channel-quality">{formatVoiceQuality(ch)}</span>
          </button>
          {#if $voiceUsers[ch.id]?.length}
            <ul class="voice-user-list">
              {#each $voiceUsers[ch.id] as user}
                <li
                  on:contextmenu={(e) => user !== $session.user && onOpenUserVolumeMenu(e, user)}
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
                  {#if $activeScreenShares[ch.id]?.includes(user) && user !== $session.user}
                    <button
                      class="screenshare-indicator"
                      on:click={() => onViewScreenShare(user)}
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
  {/each}

  <div class="voice-controls-container">
    <div class="voice-controls-panel">
      {#if inVoice}
        <div class="voice-controls-header">Voice Controls</div>
      {/if}
      <div class="voice-controls-buttons">
        {#if inVoice}
          <button class="voice-control-btn leave" on:click={onLeaveVoice}>
            <span class="btn-icon">⬅️</span>
            <span class="btn-text">Leave Voice</span>
          </button>
        {/if}
        <button
          class="voice-control-btn mute"
          class:muted={inVoice && $microphoneMuted}
          class:active={inVoice && $voiceMode === 'vad' && $voiceActivity}
          class:ptt-active={inVoice && $voiceMode === 'ptt' && $isPttActive}
          class:disabled={!inVoice}
          on:click={onToggleMicrophone}
          disabled={!inVoice}
          title={!inVoice ? 'Join a voice channel first' : $microphoneMuted ? 'Unmute Microphone' : 'Mute Microphone'}
        >
          <span class="btn-icon">
            {#if !inVoice}
              🎤
            {:else if $microphoneMuted}
              🎤🚫
            {:else if $voiceMode === 'vad' && $voiceActivity}
              🎤✨
            {:else if $voiceMode === 'ptt' && $isPttActive}
              🎤🔥
            {:else}
              🎤
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
          on:click={onToggleOutput}
          disabled={!inVoice}
          title={!inVoice ? 'Join a voice channel first' : $outputMuted ? 'Unmute Output' : 'Mute Output'}
        >
          <span class="btn-icon">{inVoice && $outputMuted ? '🔇' : '🔊'}</span>
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
  .channels {
    width: clamp(220px, 18vw, 260px);
    background: color-mix(in srgb, var(--color-surface-elevated) 86%, transparent);
    border-radius: var(--radius-lg);
    border: 1px solid var(--color-surface-outline);
    box-shadow: var(--shadow-xs);
    padding: clamp(1rem, 2vw, 1.25rem);
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    overflow-y: auto;
  }

  .channels .section {
    margin: 0.6rem 0 0.15rem 0;
    font-size: var(--text-xs);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.14em;
    color: var(--color-muted);
  }

  .category-header {
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 0.3rem;
    user-select: none;
    border-radius: var(--radius-sm, 4px);
    padding: 0.15rem 0.25rem;
    transition: background 0.15s;
  }

  .category-header:hover {
    background: color-mix(in srgb, var(--color-surface-elevated) 60%, transparent);
  }

  .category-chevron {
    display: inline-block;
    font-size: 0.6rem;
    transition: transform 0.15s ease;
    flex-shrink: 0;
  }

  .category-chevron.collapsed {
    transform: rotate(-90deg);
  }

  .channels button {
    width: 100%;
    padding: 0.5rem 0.75rem;
    border: 1px solid transparent;
    background: transparent;
    color: var(--color-muted);
    text-align: left;
    border-radius: var(--radius-sm);
    font-weight: 600;
    font-size: var(--text-md);
    letter-spacing: 0.01em;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    position: relative;
  }

  .chan-icon {
    font-size: 1rem;
    opacity: 0.65;
    width: 1.2rem;
    text-align: center;
    flex-shrink: 0;
  }

  .voice-group {
    display: flex;
    flex-direction: column;
  }

  .voice-user-list {
    list-style: none;
    margin: 0;
    padding: 0.2rem 0 0 1.6rem;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }

  .voice-user-list li {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    padding: 0.3rem 0.55rem;
    border-radius: var(--radius-xs);
    font-size: var(--text-md);
    transition: background var(--transition);
  }

  .voice-user-list li.clickable {
    cursor: context-menu;
  }

  .voice-user-list li:hover {
    background: color-mix(in srgb, var(--color-primary) 6%, transparent);
  }

  .voice-user-list li.talking {
    background: color-mix(in srgb, var(--color-success) 10%, transparent);
  }

  .voice-user-list .status.voice {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: var(--color-success);
    opacity: 0.5;
    flex-shrink: 0;
    transition: opacity 0.15s ease-in, box-shadow 0.15s ease-in;
  }

  .voice-user-list .status.voice.talking {
    opacity: 1;
    box-shadow: 0 0 6px color-mix(in srgb, var(--color-success) 50%, transparent);
    transition: opacity 0.05s ease-out, box-shadow 0.05s ease-out;
  }

  .voice-user-list .username {
    font-weight: 600;
    font-size: var(--text-sm);
    color: var(--color-on-surface);
  }

  .voice-user-list .role {
    font-size: var(--text-xs);
    font-weight: 600;
    opacity: 0.75;
  }

  .voice-channel-name {
    flex: 1;
  }

  .voice-channel-quality {
    margin-left: auto;
    font-size: var(--text-xs);
    font-weight: 600;
    color: color-mix(in srgb, var(--color-muted) 85%, transparent);
  }

  .channels button:hover {
    background: color-mix(in srgb, var(--color-primary) 8%, transparent);
    color: var(--color-on-surface);
  }

  .channels button.active {
    background: color-mix(in srgb, var(--color-primary) 18%, transparent);
    color: var(--color-on-surface);
    border-color: color-mix(in srgb, var(--color-primary) 30%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--color-primary) 24%, transparent);
  }

  .voice-controls-container {
    margin-top: auto;
    padding-top: 0.75rem;
    border-top: 1px solid var(--color-surface-outline);
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .voice-controls-panel {
    border-radius: var(--radius-md);
    padding: 0.75rem;
    background: color-mix(in srgb, var(--color-surface-raised) 86%, transparent);
    border: 1px solid var(--color-surface-outline);
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
  }

  .voice-controls-header {
    font-size: var(--text-xs);
    font-weight: 700;
    color: var(--color-muted);
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .voice-controls-buttons {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .voice-control-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    padding: 0.5rem 0.7rem;
    border-radius: var(--radius-sm);
    border: 1px solid color-mix(in srgb, var(--color-primary) 16%, transparent);
    background: color-mix(in srgb, var(--color-primary) 8%, transparent);
    color: var(--color-on-surface);
    font-weight: 600;
    font-size: var(--text-sm);
    width: 100%;
  }

  .btn-icon {
    font-size: var(--text-md);
    line-height: 1;
  }

  .btn-text {
    font-size: var(--text-sm);
  }

  .voice-control-btn:hover {
    border-color: color-mix(in srgb, var(--color-primary) 32%, transparent);
  }

  .voice-control-btn.leave {
    border-color: color-mix(in srgb, var(--color-error) 45%, transparent);
    color: var(--color-error);
    background: color-mix(in srgb, var(--color-error) 12%, transparent);
  }

  .voice-control-btn.leave:hover {
    border-color: color-mix(in srgb, var(--color-error) 60%, transparent);
  }

  .voice-control-btn.disabled {
    opacity: 0.4;
    cursor: default;
    pointer-events: none;
  }

  .voice-control-btn.mute.muted {
    background: color-mix(in srgb, var(--color-error) 15%, transparent);
    border-color: color-mix(in srgb, var(--color-error) 45%, transparent);
    color: var(--color-error);
  }

  .voice-control-btn.mute.active,
  .voice-control-btn.mute.ptt-active {
    background: color-mix(in srgb, var(--color-success) 18%, transparent);
    border-color: color-mix(in srgb, var(--color-success) 40%, transparent);
    color: var(--color-success);
  }

  .screenshare-indicator {
    background: transparent;
    border: none;
    padding: 0.25rem;
    cursor: pointer;
    color: var(--color-primary);
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-sm);
    transition: all 0.2s;
  }

  .screenshare-indicator:hover {
    background: color-mix(in srgb, var(--color-primary) 16%, transparent);
    color: var(--color-on-primary);
  }

  .screenshare-indicator svg {
    width: 1rem;
    height: 1rem;
  }
</style>
