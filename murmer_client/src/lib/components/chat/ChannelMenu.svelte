<!--
  Right-click menu on the channel list, for members with MANAGE_CHANNELS:
  creating, renaming, moving and deleting channels and categories, a voice
  channel's quality, user limit and breakout rooms, and the per-channel permissions
  editor it opens. Hiding the menu from everyone else is cosmetic; the server
  checks the permission on every one of these frames.
-->
<script lang="ts">
  import { can } from '$lib/stores/permissions';
  import { channels } from '$lib/stores/channels';
  import { voiceChannels } from '$lib/stores/voiceChannels';
  import { categories } from '$lib/stores/categories';
  import { voiceDefaults } from '$lib/stores/voiceDefaults';
  import { dialogs } from '$lib/stores/dialogs';
  import { PERMISSIONS } from '$lib/chat/permissions';
  import {
    VOICE_QUALITY_PRESETS,
    DEFAULT_VOICE_PRESET,
    MAX_VOICE_USER_LIMIT
  } from '$lib/chat/constants';
  import { parseUserLimit } from '$lib/chat/helpers';
  import type { CategoryInfo, ContextMenuItem } from '$lib/types';
  import ContextMenu from '$lib/components/ContextMenu.svelte';
  import ChannelPermissionsModal from '$lib/components/ChannelPermissionsModal.svelte';

  let menuOpen = $state(false);
  let menuX = $state(0);
  let menuY = $state(0);
  let menuChannelId: number | null = $state(null);
  let menuVoiceChannelId: number | null = $state(null);
  let menuCategoryId: number | null = $state(null);

  /** Open on a channel, or on the empty list when `channelId` is absent. */
  export function openChannel(event: MouseEvent, channelId?: number, voice?: boolean) {
    if (!$can(PERMISSIONS.MANAGE_CHANNELS)) return;
    event.preventDefault();
    event.stopPropagation();
    menuX = event.clientX;
    menuY = event.clientY;
    menuChannelId = null;
    menuVoiceChannelId = null;
    menuCategoryId = null;
    if (channelId != null) {
      if (voice) menuVoiceChannelId = channelId;
      else menuChannelId = channelId;
    }
    menuOpen = true;
  }

  /** Open on a category header. */
  export function openCategory(event: MouseEvent, category: CategoryInfo) {
    if (!$can(PERMISSIONS.MANAGE_CHANNELS)) return;
    event.preventDefault();
    event.stopPropagation();
    menuX = event.clientX;
    menuY = event.clientY;
    menuChannelId = null;
    menuVoiceChannelId = null;
    menuCategoryId = category.id;
    menuOpen = true;
  }

  // Per-channel permissions editor (private channels).
  let channelPermsOpen = $state(false);
  let channelPermsId: number | null = $state(null);
  let channelPermsVoice = $state(false);
  let channelPermsName = $state('');

  function openChannelPermissions(id: number, voice: boolean, name: string) {
    channelPermsId = id;
    channelPermsVoice = voice;
    channelPermsName = name;
    channelPermsOpen = true;
  }

  function closeChannelPermissions() {
    channelPermsOpen = false;
  }

  async function createChannelPrompt(categoryId: number | null = null, isPrivate = false) {
    const name = await dialogs.prompt({
      title: isPrivate ? 'Create private text channel' : 'Create text channel',
      label: 'Channel name',
      placeholder: 'e.g. general',
      confirmLabel: 'Create'
    });
    if (name) channels.create(name.trim(), categoryId, isPrivate);
  }

  async function selectVoicePreset(): Promise<{ quality: string; bitrate: number | null } | null> {
    const quality = await dialogs.select({
      title: 'Voice quality',
      options: VOICE_QUALITY_PRESETS.map((preset) => ({
        value: preset.quality,
        label: preset.label,
        description:
          preset.bitrate && preset.bitrate > 0
            ? `${Math.round(preset.bitrate / 1000)} kbps`
            : 'Uncompressed audio'
      })),
      // The server's configured default, so a channel created without a
      // thought still lands on what the operator wanted.
      initial: $voiceDefaults.quality,
      confirmLabel: 'Apply'
    });
    if (quality === null) return null;
    const preset = VOICE_QUALITY_PRESETS.find((p) => p.quality === quality) ?? DEFAULT_VOICE_PRESET;
    return { quality: preset.quality, bitrate: preset.bitrate };
  }

  async function createVoiceChannelPrompt(categoryId: number | null = null, isPrivate = false) {
    const name = await dialogs.prompt({
      title: isPrivate ? 'Create private voice channel' : 'Create voice channel',
      label: 'Channel name',
      placeholder: 'e.g. Lounge',
      confirmLabel: 'Next'
    });
    if (!name) return;
    const preset = await selectVoicePreset();
    if (!preset) return;
    voiceChannels.create(name.trim(), preset, categoryId, isPrivate);
  }

  async function createCategoryPrompt() {
    const name = await dialogs.prompt({
      title: 'Create category',
      label: 'Category name',
      placeholder: 'e.g. Projects',
      confirmLabel: 'Create'
    });
    if (name) categories.create(name.trim());
  }

  async function renameCategoryPrompt(id: number) {
    const cat = $categories.find((c) => c.id === id);
    const name = await dialogs.prompt({
      title: 'Rename category',
      label: 'Category name',
      initial: cat?.name ?? '',
      confirmLabel: 'Rename'
    });
    if (name) categories.rename(id, name.trim());
  }

  async function renameChannelPrompt(id: number) {
    const ch = $channels.find((c) => c.id === id);
    const name = await dialogs.prompt({
      title: 'Rename channel',
      label: 'Channel name',
      initial: ch?.name ?? '',
      confirmLabel: 'Rename'
    });
    if (name) channels.rename(id, name.trim());
  }

  async function renameVoiceChannelPrompt(id: number) {
    const ch = $voiceChannels.find((c) => c.id === id);
    const name = await dialogs.prompt({
      title: 'Rename voice channel',
      label: 'Channel name',
      initial: ch?.name ?? '',
      confirmLabel: 'Rename'
    });
    if (name) voiceChannels.rename(id, name.trim());
  }

  async function setUserLimitPrompt(id: number) {
    const ch = $voiceChannels.find((c) => c.id === id);
    const text = await dialogs.prompt({
      title: 'Set user limit',
      message: `How many members may be in the channel at once, up to ${MAX_VOICE_USER_LIMIT}. 0 or empty removes the limit. Nobody already inside is moved out.`,
      label: 'User limit',
      initial: ch?.userLimit ? String(ch.userLimit) : '',
      maxLength: 2,
      confirmLabel: 'Save',
      required: false
    });
    if (text === null) return;
    const limit = parseUserLimit(text);
    if (limit === null) {
      await dialogs.alert({
        title: 'Invalid user limit',
        message: `Enter a whole number from 0 to ${MAX_VOICE_USER_LIMIT}.`
      });
      return;
    }
    voiceChannels.setUserLimit(id, limit);
  }

  /** Builds the "Move to" submenu; empty when there is nowhere to move to. */
  function buildMoveToItems(channelId: number, voice: boolean): ContextMenuItem[] {
    const targets: ContextMenuItem[] = [];
    const currentCh = voice
      ? $voiceChannels.find((c) => c.id === channelId)
      : $channels.find((c) => c.id === channelId);
    const currentCatId = currentCh?.categoryId ?? null;

    if (currentCatId !== null) {
      targets.push({
        label: '(no category)',
        action: () => channels.move(channelId, null, voice)
      });
    }

    for (const cat of $categories) {
      if (cat.id !== currentCatId) {
        targets.push({
          label: cat.name,
          action: () => channels.move(channelId, cat.id, voice)
        });
      }
    }

    return targets.length ? [{ label: 'Move to', children: targets }] : [];
  }

  /* Room counts offered when splitting a call. Not a mirror of the server's
     cap — that one is the authority and validates every request; this is the
     handful of splits worth one click. */
  const BREAKOUT_ROOM_CHOICES = [2, 3, 4, 5, 6];

  /**
   * Builds the breakout entry for a voice channel: either splitting it, or
   * closing the split it is part of. A channel is never both.
   */
  function buildBreakoutItems(channelId: number): ContextMenuItem[] {
    const channel = $voiceChannels.find((c) => c.id === channelId);
    if (!channel) return [];
    const openOn = channel.breakoutParent ?? channelId;
    const splitIsOpen =
      channel.breakoutParent != null || $voiceChannels.some((c) => c.breakoutParent === channelId);
    if (splitIsOpen) {
      return [
        {
          label: 'Close Breakout Rooms',
          action: () => voiceChannels.closeBreakouts(openOn)
        }
      ];
    }
    return [
      {
        label: 'Split into Breakout Rooms',
        children: BREAKOUT_ROOM_CHOICES.map((rooms) => ({
          label: `${rooms} rooms`,
          action: () => voiceChannels.openBreakouts(channelId, rooms)
        }))
      }
    ];
  }

  let items = $derived(!$can(PERMISSIONS.MANAGE_CHANNELS) ? [] : [
    {
      label: 'Create',
      children: [
        { label: 'Text Channel', action: () => createChannelPrompt() },
        { label: 'Voice Channel', action: () => createVoiceChannelPrompt() },
        { label: 'Private Text Channel', action: () => createChannelPrompt(null, true) },
        { label: 'Private Voice Channel', action: () => createVoiceChannelPrompt(null, true) },
        { label: 'Category', action: createCategoryPrompt }
      ]
    },
    ...(menuChannelId != null
      ? [
          {
            label: 'Edit Permissions',
            action: () =>
              openChannelPermissions(
                menuChannelId!,
                false,
                $channels.find((c) => c.id === menuChannelId)?.name ?? ''
              )
          },
          { label: 'Rename Channel', action: () => renameChannelPrompt(menuChannelId!) },
          ...buildMoveToItems(menuChannelId, false),
          { label: 'Delete Channel', action: () => channels.remove(menuChannelId!), danger: true }
        ]
      : []),
    ...(menuVoiceChannelId != null
      ? [
          {
            label: 'Edit Permissions',
            action: () =>
              openChannelPermissions(
                menuVoiceChannelId!,
                true,
                $voiceChannels.find((c) => c.id === menuVoiceChannelId)?.name ?? ''
              )
          },
          {
            label: 'Set Voice Quality',
            children: VOICE_QUALITY_PRESETS.map((preset) => ({
              label:
                preset.bitrate && preset.bitrate > 0
                  ? `${preset.label} (${Math.round(preset.bitrate / 1000)} kbps)`
                  : preset.label,
              action: () =>
                voiceChannels.configure(menuVoiceChannelId!, {
                  quality: preset.quality,
                  bitrate: preset.bitrate
                })
            }))
          },
          { label: 'Set User Limit', action: () => setUserLimitPrompt(menuVoiceChannelId!) },
          ...buildBreakoutItems(menuVoiceChannelId),
          { label: 'Rename Voice Channel', action: () => renameVoiceChannelPrompt(menuVoiceChannelId!) },
          ...buildMoveToItems(menuVoiceChannelId, true),
          { label: 'Delete Voice Channel', action: () => voiceChannels.remove(menuVoiceChannelId!), danger: true }
        ]
      : []),
    ...(menuCategoryId != null
      ? [
          { label: 'Create Text Channel Here', action: () => createChannelPrompt(menuCategoryId) },
          { label: 'Create Voice Channel Here', action: () => createVoiceChannelPrompt(menuCategoryId) },
          { label: 'Rename Category', action: () => renameCategoryPrompt(menuCategoryId!) },
          { label: 'Delete Category', action: () => categories.remove(menuCategoryId!), danger: true }
        ]
      : [])
  ]);
</script>

<ContextMenu bind:open={menuOpen} x={menuX} y={menuY} {items} />
<ChannelPermissionsModal
  open={channelPermsOpen}
  close={closeChannelPermissions}
  channelId={channelPermsId}
  voice={channelPermsVoice}
  channelName={channelPermsName}
/>
