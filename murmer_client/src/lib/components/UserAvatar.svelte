<!--
  Initials avatar with a deterministic per-user hue, so the same name always
  gets the same color on every client. Sizes follow the spacing scale.
-->
<script lang="ts">
  interface Props {
    name: string;
    size?: 'sm' | 'md';
  }

  let { name, size = 'md' }: Props = $props();

  /* FNV-1a over the name, mapped onto the hue circle. */
  function hueFor(value: string): number {
    let hash = 0x811c9dc5;
    for (let i = 0; i < value.length; i += 1) {
      hash ^= value.charCodeAt(i);
      hash = Math.imul(hash, 0x01000193);
    }
    return (hash >>> 0) % 360;
  }

  let hue = $derived(hueFor(name));
  let initials = $derived(name.trim().slice(0, 2).toUpperCase() || '??');
</script>

<span class="avatar {size}" style="--avatar-hue: {hue}" aria-hidden="true">{initials}</span>

<style>
  .avatar {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    border-radius: var(--radius-pill);
    font-weight: 600;
    user-select: none;
    background: hsl(var(--avatar-hue) 35% 30%);
    color: hsl(var(--avatar-hue) 70% 88%);
  }

  :global(html[data-theme='light']) .avatar {
    background: hsl(var(--avatar-hue) 50% 86%);
    color: hsl(var(--avatar-hue) 55% 28%);
  }

  .avatar.md {
    width: var(--space-6);
    height: var(--space-6);
    font-size: var(--text-sm);
  }

  .avatar.sm {
    width: var(--space-5);
    height: var(--space-5);
    font-size: var(--text-xs);
  }
</style>
