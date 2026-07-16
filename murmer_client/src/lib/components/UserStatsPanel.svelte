<!--
  Lifetime stats and achievements for one user, rendered from a stats
  snapshot (see the stats store). Used inside the settings modal; the panel
  itself is display-only — opt-in and purge controls live with the caller.
-->
<script lang="ts">
  import type { UserStatsSnapshot } from '$lib/stores/stats';
  import { computeAchievements, type AchievementIcon } from '$lib/stats/achievements';
  import { customEmojis } from '$lib/stores/customEmojis';
  import { selectedServer } from '$lib/stores/servers';
  import { httpBaseFromWs } from '$lib/server-url';
  import { describeDuration } from '$lib/chat/helpers';

  interface Props {
    snapshot: UserStatsSnapshot;
  }

  let { snapshot }: Props = $props();

  let httpBase = $derived($selectedServer ? httpBaseFromWs($selectedServer) : '');
  let achievements = $derived(computeAchievements(snapshot.stats));
  let unlockedCount = $derived(achievements.filter((a) => a.tierIndex >= 0).length);

  const numberFormat = new Intl.NumberFormat();

  function formatNumber(value: number): string {
    return numberFormat.format(value);
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    const units = ['KB', 'MB', 'GB', 'TB'];
    let value = bytes;
    let unit = 'B';
    for (const next of units) {
      if (value < 1024) break;
      value /= 1024;
      unit = next;
    }
    return `${value >= 100 ? Math.round(value) : value.toFixed(1)} ${unit}`;
  }

  function formatDuration(seconds: number): string {
    if (seconds <= 0) return '0 minutes';
    return describeDuration(seconds);
  }

  function formatDate(iso: string | null): string | null {
    if (!iso) return null;
    const parsed = new Date(iso);
    return Number.isNaN(parsed.getTime()) ? null : parsed.toLocaleDateString();
  }

  /** Custom emoji shortcodes (`:name:`) resolve to their uploaded image. */
  function customEmojiUrl(emoji: string): string | null {
    const match = emoji.match(/^:([a-z0-9_]+):$/);
    if (!match) return null;
    const entry = $customEmojis[match[1]];
    return entry ? httpBase + entry.url : null;
  }

  let statRows = $derived([
    { label: 'Messages sent', value: formatNumber(snapshot.stats.messagesSent) },
    { label: 'Characters typed', value: formatNumber(snapshot.stats.messageChars) },
    { label: 'Message data written', value: formatBytes(snapshot.stats.messageBytes) },
    { label: 'Longest message', value: `${formatNumber(snapshot.stats.longestMessageChars)} chars` },
    { label: 'Pictures shared', value: formatNumber(snapshot.stats.imagesSent) },
    { label: 'GIFs sent', value: formatNumber(snapshot.stats.gifsSent) },
    { label: 'Files sent', value: formatNumber(snapshot.stats.filesSent) },
    { label: 'Data uploaded', value: formatBytes(snapshot.stats.uploadBytes) },
    { label: 'Links shared', value: formatNumber(snapshot.stats.linksShared) },
    { label: 'Replies sent', value: formatNumber(snapshot.stats.repliesSent) },
    { label: 'People mentioned', value: formatNumber(snapshot.stats.mentionsSent) },
    { label: 'Direct messages sent', value: formatNumber(snapshot.stats.dmsSent) },
    { label: 'Reactions given', value: formatNumber(snapshot.stats.reactionsGiven) },
    { label: 'Reactions received', value: formatNumber(snapshot.stats.reactionsReceived) },
    { label: 'Messages edited', value: formatNumber(snapshot.stats.messagesEdited) },
    { label: 'Messages deleted', value: formatNumber(snapshot.stats.messagesDeleted) },
    { label: 'Messages pinned', value: formatNumber(snapshot.stats.pinsAdded) },
    { label: 'Time in voice', value: formatDuration(snapshot.stats.voiceSeconds) },
    { label: 'Voice sessions', value: formatNumber(snapshot.stats.voiceSessions) },
    { label: 'Time screen sharing', value: formatDuration(snapshot.stats.screenshareSeconds) }
  ]);

  // Small stroke-SVG icon set (1.8 stroke width, matching the app style).
  const ICON_PATHS: Record<AchievementIcon, string> = {
    message: 'M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z',
    text: 'M4 7V5h16v2 M9 20h6 M12 5v15',
    image:
      'M3 5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z M8.5 10a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3 M21 15l-5-5L5 21',
    sparkle: 'M12 3l1.9 5.8L20 10l-6.1 1.2L12 17l-1.9-5.8L4 10l6.1-1.2z M19 17l.7 2.3L22 20l-2.3.7L19 23l-.7-2.3L16 20l2.3-.7z',
    upload: 'M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4 M17 8l-5-5-5 5 M12 3v12',
    link: 'M10 13a5 5 0 0 0 7.5.5l3-3a5 5 0 0 0-7-7l-1.7 1.7 M14 11a5 5 0 0 0-7.5-.5l-3 3a5 5 0 0 0 7 7l1.7-1.7',
    reply: 'M9 17l-5-5 5-5 M4 12h11a5 5 0 0 1 5 5v2',
    mail: 'M4 4h16a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z M22 6l-10 7L2 6',
    heart:
      'M20.8 4.6a5.5 5.5 0 0 0-7.8 0L12 5.6l-1-1a5.5 5.5 0 0 0-7.8 7.8l1 1L12 21.2l7.8-7.8 1-1a5.5 5.5 0 0 0 0-7.8z',
    star: 'M12 2l3.1 6.3 6.9 1-5 4.9 1.2 6.8L12 17.8 5.8 21l1.2-6.8-5-4.9 6.9-1z',
    edit: 'M17 3a2.8 2.8 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5z',
    trash: 'M3 6h18 M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2 M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6',
    pin: 'M12 17v5 M9 3h6l1 7 2 2H6l2-2z',
    mic: 'M12 2a3 3 0 0 1 3 3v6a3 3 0 0 1-6 0V5a3 3 0 0 1 3-3z M19 10v1a7 7 0 0 1-14 0v-1 M12 18v4',
    monitor: 'M2 4h20v12H2z M8 20h8 M12 16v4',
    at: 'M12 16a4 4 0 1 0 0-8 4 4 0 0 0 0 8z M16 8v5a3 3 0 0 0 6 0v-1a10 10 0 1 0-3.9 7.9',
    zap: 'M13 2L3 14h9l-1 8 10-12h-9z'
  };
</script>

<div class="stats-panel">
  <div class="stats-meta">
    {#if formatDate(snapshot.trackedSince)}
      <span>Tracking since {formatDate(snapshot.trackedSince)}</span>
    {:else}
      <span>Nothing recorded yet — stats appear as you use the server.</span>
    {/if}
    <span class="badge">{unlockedCount}/{achievements.length} achievements</span>
  </div>

  {#if snapshot.favoriteReactions.length > 0}
    <div class="favorite-reactions">
      <span class="favorites-label">Favorite reactions</span>
      <div class="favorites-row">
        {#each snapshot.favoriteReactions as favorite (favorite.emoji)}
          <span class="favorite" title={`Used ${formatNumber(favorite.count)} times`}>
            {#if customEmojiUrl(favorite.emoji)}
              <img src={customEmojiUrl(favorite.emoji)} alt={favorite.emoji} width="18" height="18" />
            {:else}
              <span class="favorite-emoji">{favorite.emoji}</span>
            {/if}
            <span class="favorite-count">{formatNumber(favorite.count)}</span>
          </span>
        {/each}
      </div>
    </div>
  {/if}

  <div class="stat-grid">
    {#each statRows as row (row.label)}
      <div class="stat-cell">
        <span class="stat-value">{row.value}</span>
        <span class="stat-label">{row.label}</span>
      </div>
    {/each}
  </div>

  <div class="achievements">
    <span class="favorites-label">Achievements</span>
    <ul class="achievement-list">
      {#each achievements as entry (entry.def.id)}
        <li class="achievement" class:unlocked={entry.tierIndex >= 0}>
          <span class="achievement-icon" aria-hidden="true">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
              <path d={ICON_PATHS[entry.def.icon]}></path>
            </svg>
          </span>
          <span class="achievement-text">
            <span class="achievement-name">
              {entry.tierIndex >= 0 ? entry.def.tiers[entry.tierIndex].name : 'Locked'}
              {#if entry.def.tiers.length > 1}
                <span class="achievement-tier">
                  {Math.max(entry.tierIndex + 1, 0)}/{entry.def.tiers.length}
                </span>
              {/if}
            </span>
            <span class="achievement-desc">
              {entry.def.description}
              {#if entry.nextTier}
                — next at {formatNumber(entry.nextTier.threshold)}
              {/if}
            </span>
          </span>
          <span class="achievement-progress">
            <span class="progress-track">
              <span class="progress-fill" style={`width: ${Math.round(entry.progress * 100)}%`}></span>
            </span>
            <span class="progress-value">{formatNumber(entry.value)}</span>
          </span>
        </li>
      {/each}
    </ul>
  </div>
</div>

<style>
  .stats-panel {
    display: grid;
    gap: var(--space-4);
  }

  .stats-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    color: var(--color-muted);
    font-size: var(--text-sm);
  }

  .favorite-reactions,
  .achievements {
    display: grid;
    gap: var(--space-2);
  }

  .favorites-label {
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--color-muted);
  }

  .favorites-row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .favorite {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-md);
    background: var(--color-surface-raised);
    border: 1px solid var(--color-surface-outline);
  }

  .favorite img {
    width: 1.125rem;
    height: 1.125rem;
    object-fit: contain;
  }

  .favorite-emoji {
    font-size: var(--text-md);
    line-height: 1;
  }

  .favorite-count {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .stat-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(9.5rem, 1fr));
    gap: var(--space-2);
  }

  .stat-cell {
    display: grid;
    gap: 0.125rem;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    background: var(--color-surface-raised);
    border: 1px solid var(--color-surface-outline);
  }

  .stat-value {
    font-family: var(--font-mono);
    font-size: var(--text-md);
    font-weight: 600;
    color: var(--color-on-surface);
  }

  .stat-label {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .achievement-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: var(--space-2);
  }

  .achievement {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    background: var(--color-surface-raised);
    border: 1px solid var(--color-surface-outline);
  }

  .achievement-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 2.25rem;
    height: 2.25rem;
    flex-shrink: 0;
    border-radius: var(--radius-md);
    color: var(--color-muted);
    background: var(--color-surface);
    border: 1px solid var(--color-surface-outline);
  }

  .achievement.unlocked .achievement-icon {
    color: var(--color-primary);
    background: var(--color-primary-container);
    border-color: transparent;
  }

  .achievement-text {
    display: grid;
    gap: 0.125rem;
    min-width: 0;
    flex: 1;
  }

  .achievement-name {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-weight: 600;
    font-size: var(--text-md);
    color: var(--color-on-surface);
  }

  .achievement:not(.unlocked) .achievement-name {
    color: var(--color-muted);
  }

  .achievement-tier {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    font-weight: 400;
    color: var(--color-muted);
  }

  .achievement-desc {
    font-size: var(--text-sm);
    color: var(--color-muted);
  }

  .achievement-progress {
    display: grid;
    gap: 0.125rem;
    justify-items: end;
    width: 7rem;
    flex-shrink: 0;
  }

  .progress-track {
    width: 100%;
    height: 0.375rem;
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    border: 1px solid var(--color-surface-outline);
    overflow: hidden;
  }

  .progress-fill {
    display: block;
    height: 100%;
    background: var(--color-primary);
    border-radius: inherit;
  }

  .progress-value {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--color-muted);
  }
</style>
