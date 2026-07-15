import type { UserStats } from '$lib/stores/stats';

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
    description: 'Messages sent',
    tiers: [
      { name: 'First Words', threshold: 1 },
      { name: 'Conversationalist', threshold: 100 },
      { name: 'Chatterbox', threshold: 1_000 },
      { name: 'Town Crier', threshold: 10_000 }
    ]
  },
  {
    id: 'chars',
    stat: 'messageChars',
    icon: 'text',
    description: 'Characters typed',
    tiers: [
      { name: 'Scribbler', threshold: 1_000 },
      { name: 'Essayist', threshold: 25_000 },
      { name: 'Novelist', threshold: 250_000 }
    ]
  },
  {
    id: 'longest',
    stat: 'longestMessageChars',
    icon: 'zap',
    description: 'Longest single message',
    tiers: [
      { name: 'Getting Wordy', threshold: 500 },
      { name: 'Wall of Text', threshold: 2_000 }
    ]
  },
  {
    id: 'images',
    stat: 'imagesSent',
    icon: 'image',
    description: 'Pictures shared',
    tiers: [
      { name: 'Snapshot', threshold: 1 },
      { name: 'Shutterbug', threshold: 50 },
      { name: 'Gallery Curator', threshold: 500 }
    ]
  },
  {
    id: 'gifs',
    stat: 'gifsSent',
    icon: 'sparkle',
    description: 'GIFs sent',
    tiers: [
      { name: 'Animated', threshold: 1 },
      { name: 'GIF Enthusiast', threshold: 25 },
      { name: 'GIF Wizard', threshold: 250 }
    ]
  },
  {
    id: 'uploads',
    stat: 'uploadBytes',
    icon: 'upload',
    description: 'Bytes uploaded',
    tiers: [
      { name: 'Courier', threshold: 10 * MB },
      { name: 'Heavy Lifter', threshold: 100 * MB },
      { name: 'Data Hauler', threshold: 1024 * MB }
    ]
  },
  {
    id: 'links',
    stat: 'linksShared',
    icon: 'link',
    description: 'Links shared',
    tiers: [
      { name: 'Referrer', threshold: 10 },
      { name: 'Link Curator', threshold: 100 }
    ]
  },
  {
    id: 'replies',
    stat: 'repliesSent',
    icon: 'reply',
    description: 'Replies sent',
    tiers: [
      { name: 'In Context', threshold: 10 },
      { name: 'Threadweaver', threshold: 250 }
    ]
  },
  {
    id: 'mentions',
    stat: 'mentionsSent',
    icon: 'at',
    description: 'People mentioned',
    tiers: [
      { name: 'Name Dropper', threshold: 10 },
      { name: 'Ping Machine', threshold: 250 }
    ]
  },
  {
    id: 'dms',
    stat: 'dmsSent',
    icon: 'mail',
    description: 'Direct messages sent',
    tiers: [
      { name: 'Pen Pal', threshold: 10 },
      { name: 'Social Butterfly', threshold: 250 }
    ]
  },
  {
    id: 'reactions-given',
    stat: 'reactionsGiven',
    icon: 'heart',
    description: 'Reactions given',
    tiers: [
      { name: 'Appreciator', threshold: 10 },
      { name: 'Cheerleader', threshold: 250 },
      { name: 'Hype Engine', threshold: 2_500 }
    ]
  },
  {
    id: 'reactions-received',
    stat: 'reactionsReceived',
    icon: 'star',
    description: 'Reactions received',
    tiers: [
      { name: 'Noticed', threshold: 10 },
      { name: 'Crowd Favorite', threshold: 250 },
      { name: 'Server Legend', threshold: 2_500 }
    ]
  },
  {
    id: 'edits',
    stat: 'messagesEdited',
    icon: 'edit',
    description: 'Messages edited',
    tiers: [
      { name: 'Second Thoughts', threshold: 10 },
      { name: 'Perfectionist', threshold: 100 }
    ]
  },
  {
    id: 'deletes',
    stat: 'messagesDeleted',
    icon: 'trash',
    description: 'Own messages deleted',
    tiers: [
      { name: 'Ctrl+Z', threshold: 10 },
      { name: 'Revisionist', threshold: 100 }
    ]
  },
  {
    id: 'pins',
    stat: 'pinsAdded',
    icon: 'pin',
    description: 'Messages pinned',
    tiers: [
      { name: 'Bookmarker', threshold: 5 },
      { name: 'Archivist', threshold: 50 }
    ]
  },
  {
    id: 'voice',
    stat: 'voiceSeconds',
    icon: 'mic',
    description: 'Time in voice chat',
    tiers: [
      { name: 'Mic Check', threshold: 10 * MINUTE },
      { name: 'On Air', threshold: 10 * HOUR },
      { name: 'Radio Host', threshold: 100 * HOUR }
    ]
  },
  {
    id: 'screenshare',
    stat: 'screenshareSeconds',
    icon: 'monitor',
    description: 'Time screen sharing',
    tiers: [
      { name: 'Presenter', threshold: 10 * MINUTE },
      { name: 'Director', threshold: 10 * HOUR }
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
