import type { UserStats } from '$lib/stores/stats';
import { t } from '$lib/i18n';

/**
 * Achievement definitions derived from lifetime stats. Everything here is
 * computed client-side from the stats snapshot the server returns — the
 * server only stores raw counters.
 *
 * Icons reference the small stroke-SVG set rendered by `UserStatsPanel`.
 */

export type AchievementIcon =
  | 'message'
  | 'text'
  | 'image'
  | 'sparkle'
  | 'upload'
  | 'link'
  | 'reply'
  | 'mail'
  | 'heart'
  | 'star'
  | 'edit'
  | 'trash'
  | 'pin'
  | 'mic'
  | 'monitor'
  | 'at'
  | 'zap';

export interface AchievementTier {
  /** Name shown once the tier is reached. */
  name: string;
  threshold: number;
}

export interface AchievementDef {
  id: string;
  /** Which lifetime counter drives progress. */
  stat: keyof UserStats;
  icon: AchievementIcon;
  description: string;
  /** Ascending thresholds; the highest reached tier is displayed. */
  tiers: AchievementTier[];
}

export interface AchievementProgress {
  def: AchievementDef;
  /** Current raw counter value. */
  value: number;
  /** Index of the highest reached tier, or -1 when none is reached yet. */
  tierIndex: number;
  /** The tier currently worked towards, or null when everything is done. */
  nextTier: AchievementTier | null;
  /** Progress towards `nextTier` in [0, 1]; 1 when all tiers are complete. */
  progress: number;
}

const MINUTE = 60;
const HOUR = 3600;
const MB = 1024 * 1024;

export const ACHIEVEMENTS: AchievementDef[] = [
  {
    id: 'messages',
    stat: 'messagesSent',
    icon: 'message',
    description: t('achievement.messages.description'),
    tiers: [
      { name: t('achievement.messages.tier0'), threshold: 1 },
      { name: t('achievement.messages.tier1'), threshold: 100 },
      { name: t('achievement.messages.tier2'), threshold: 1_000 },
      { name: t('achievement.messages.tier3'), threshold: 10_000 }
    ]
  },
  {
    id: 'chars',
    stat: 'messageChars',
    icon: 'text',
    description: t('achievement.chars.description'),
    tiers: [
      { name: t('achievement.chars.tier0'), threshold: 1_000 },
      { name: t('achievement.chars.tier1'), threshold: 25_000 },
      { name: t('achievement.chars.tier2'), threshold: 250_000 }
    ]
  },
  {
    id: 'longest',
    stat: 'longestMessageChars',
    icon: 'zap',
    description: t('achievement.longest.description'),
    tiers: [
      { name: t('achievement.longest.tier0'), threshold: 500 },
      { name: t('achievement.longest.tier1'), threshold: 2_000 }
    ]
  },
  {
    id: 'images',
    stat: 'imagesSent',
    icon: 'image',
    description: t('achievement.images.description'),
    tiers: [
      { name: t('achievement.images.tier0'), threshold: 1 },
      { name: t('achievement.images.tier1'), threshold: 50 },
      { name: t('achievement.images.tier2'), threshold: 500 }
    ]
  },
  {
    id: 'gifs',
    stat: 'gifsSent',
    icon: 'sparkle',
    description: t('achievement.gifs.description'),
    tiers: [
      { name: t('achievement.gifs.tier0'), threshold: 1 },
      { name: t('achievement.gifs.tier1'), threshold: 25 },
      { name: t('achievement.gifs.tier2'), threshold: 250 }
    ]
  },
  {
    id: 'uploads',
    stat: 'uploadBytes',
    icon: 'upload',
    description: t('achievement.uploads.description'),
    tiers: [
      { name: t('achievement.uploads.tier0'), threshold: 10 * MB },
      { name: t('achievement.uploads.tier1'), threshold: 100 * MB },
      { name: t('achievement.uploads.tier2'), threshold: 1024 * MB }
    ]
  },
  {
    id: 'links',
    stat: 'linksShared',
    icon: 'link',
    description: t('achievement.links.description'),
    tiers: [
      { name: t('achievement.links.tier0'), threshold: 10 },
      { name: t('achievement.links.tier1'), threshold: 100 }
    ]
  },
  {
    id: 'replies',
    stat: 'repliesSent',
    icon: 'reply',
    description: t('achievement.replies.description'),
    tiers: [
      { name: t('achievement.replies.tier0'), threshold: 10 },
      { name: t('achievement.replies.tier1'), threshold: 250 }
    ]
  },
  {
    id: 'mentions',
    stat: 'mentionsSent',
    icon: 'at',
    description: t('achievement.mentions.description'),
    tiers: [
      { name: t('achievement.mentions.tier0'), threshold: 10 },
      { name: t('achievement.mentions.tier1'), threshold: 250 }
    ]
  },
  {
    id: 'dms',
    stat: 'dmsSent',
    icon: 'mail',
    description: t('achievement.dms.description'),
    tiers: [
      { name: t('achievement.dms.tier0'), threshold: 10 },
      { name: t('achievement.dms.tier1'), threshold: 250 }
    ]
  },
  {
    id: 'reactions-given',
    stat: 'reactionsGiven',
    icon: 'heart',
    description: t('achievement.reactionsGiven.description'),
    tiers: [
      { name: t('achievement.reactionsGiven.tier0'), threshold: 10 },
      { name: t('achievement.reactionsGiven.tier1'), threshold: 250 },
      { name: t('achievement.reactionsGiven.tier2'), threshold: 2_500 }
    ]
  },
  {
    id: 'reactions-received',
    stat: 'reactionsReceived',
    icon: 'star',
    description: t('achievement.reactionsReceived.description'),
    tiers: [
      { name: t('achievement.reactionsReceived.tier0'), threshold: 10 },
      { name: t('achievement.reactionsReceived.tier1'), threshold: 250 },
      { name: t('achievement.reactionsReceived.tier2'), threshold: 2_500 }
    ]
  },
  {
    id: 'edits',
    stat: 'messagesEdited',
    icon: 'edit',
    description: t('achievement.edits.description'),
    tiers: [
      { name: t('achievement.edits.tier0'), threshold: 10 },
      { name: t('achievement.edits.tier1'), threshold: 100 }
    ]
  },
  {
    id: 'deletes',
    stat: 'messagesDeleted',
    icon: 'trash',
    description: t('achievement.deletes.description'),
    tiers: [
      { name: t('achievement.deletes.tier0'), threshold: 10 },
      { name: t('achievement.deletes.tier1'), threshold: 100 }
    ]
  },
  {
    id: 'pins',
    stat: 'pinsAdded',
    icon: 'pin',
    description: t('achievement.pins.description'),
    tiers: [
      { name: t('achievement.pins.tier0'), threshold: 5 },
      { name: t('achievement.pins.tier1'), threshold: 50 }
    ]
  },
  {
    id: 'voice',
    stat: 'voiceSeconds',
    icon: 'mic',
    description: t('achievement.voice.description'),
    tiers: [
      { name: t('achievement.voice.tier0'), threshold: 10 * MINUTE },
      { name: t('achievement.voice.tier1'), threshold: 10 * HOUR },
      { name: t('achievement.voice.tier2'), threshold: 100 * HOUR }
    ]
  },
  {
    id: 'screenshare',
    stat: 'screenshareSeconds',
    icon: 'monitor',
    description: t('achievement.screenshare.description'),
    tiers: [
      { name: t('achievement.screenshare.tier0'), threshold: 10 * MINUTE },
      { name: t('achievement.screenshare.tier1'), threshold: 10 * HOUR }
    ]
  },
  {
    id: 'soundboard',
    stat: 'soundsPlayed',
    icon: 'zap',
    description: t('achievement.soundboard.description'),
    tiers: [
      { name: t('achievement.soundboard.tier0'), threshold: 10 },
      { name: t('achievement.soundboard.tier1'), threshold: 250 },
      { name: t('achievement.soundboard.tier2'), threshold: 2500 }
    ]
  }
];

/** Compute progress for every achievement from a stats snapshot. */
export function computeAchievements(stats: UserStats): AchievementProgress[] {
  return ACHIEVEMENTS.map((def) => {
    const value = stats[def.stat] ?? 0;
    let tierIndex = -1;
    for (let i = 0; i < def.tiers.length; i++) {
      if (value >= def.tiers[i].threshold) tierIndex = i;
    }
    const nextTier = tierIndex + 1 < def.tiers.length ? def.tiers[tierIndex + 1] : null;
    const progress = nextTier ? Math.min(value / nextTier.threshold, 1) : 1;
    return { def, value, tierIndex, nextTier, progress };
  });
}
