<!--
  The icon of a user's display role, rendered inline next to their name in the
  member lists and on messages. The source is always a `/files/<key>` upload on
  the connected server (a custom emoji or an image uploaded in the Server
  Dashboard), resolved against the same HTTP base as avatars and emojis.
  Renders nothing when the role has no icon or the image fails to load.
-->
<script lang="ts">
  import { selectedServer } from '$lib/stores/servers';
  import { httpBaseFromWs } from '$lib/server-url';

  interface Props {
    /** Role icon URL as stored on the server, e.g. `/files/badge.png`. */
    icon?: string;
    /** Role name; used as the tooltip and alt text. */
    role?: string;
    size?: 'sm' | 'md';
  }

  let { icon, role, size = 'sm' }: Props = $props();

  let src = $derived.by(() => {
    if (!icon || !$selectedServer) return null;
    return httpBaseFromWs($selectedServer) + icon;
  });
  /* A broken image hides the badge; a changed URL is retried. */
  let failedSrc: string | null = $state(null);
</script>

{#if src && failedSrc !== src}
  <img
    class="role-icon {size}"
    {src}
    alt={role ?? ''}
    title={role}
    loading="lazy"
    onerror={() => (failedSrc = src)}
  />
{/if}

<style>
  .role-icon {
    flex-shrink: 0;
    object-fit: contain;
    vertical-align: -0.15em;
  }

  .role-icon.sm {
    width: var(--space-4);
    height: var(--space-4);
  }

  .role-icon.md {
    width: var(--space-5);
    height: var(--space-5);
  }
</style>
