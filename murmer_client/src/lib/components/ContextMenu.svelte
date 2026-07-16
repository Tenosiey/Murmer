<!--
  Modern floating context menu with smooth animations and micro-interactions.
  Accepts a list of menu items and exposes coordinates for positioning relative
  to the user's cursor. Items carrying `children` open a flyout submenu on
  hover or click. Features fade-in/scale animation and keyboard navigation.
-->
<script lang="ts">

  import { onMount, onDestroy, tick } from 'svelte';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import type { ContextMenuItem } from '$lib/types';

  interface Props {
    items?: ContextMenuItem[];
    x?: number;
    y?: number;
    open?: boolean;
  }

  let {
    items = [],
    x = 0,
    y = 0,
    open = $bindable(false)
  }: Props = $props();

  let menuElement: HTMLUListElement | undefined = $state();
  let submenuElement: HTMLUListElement | undefined = $state();

  /** Index of the item whose submenu is currently shown, if any. */
  let openSubmenu: number | null = $state(null);
  /** Submenu opens to the left when it would leave the viewport on the right. */
  let submenuFlip = $state(false);
  /** Pixels the submenu is pulled up by to stay inside the viewport. */
  let submenuShift = $state(0);
  /** Grace period so the cursor may cross the gap between the menu and its
   *  submenu without the submenu disappearing underneath it. */
  let submenuCloseTimer: ReturnType<typeof setTimeout> | null = null;

  /** A menu with submenus must not clip them, so it cannot be a scroll box.
   *  Keep such menus short enough to fit on screen. */
  let hasSubmenu = $derived(items.some((item) => item.children?.length));

  function cancelSubmenuClose() {
    if (submenuCloseTimer !== null) {
      clearTimeout(submenuCloseTimer);
      submenuCloseTimer = null;
    }
  }

  function closeSubmenu() {
    cancelSubmenuClose();
    openSubmenu = null;
  }

  function scheduleSubmenuClose() {
    cancelSubmenuClose();
    submenuCloseTimer = setTimeout(() => {
      openSubmenu = null;
      submenuCloseTimer = null;
    }, 150);
  }

  /** Opens the submenu of `index`, then nudges it back inside the viewport.
   *  It is positioned against its parent item rather than the viewport so the
   *  menu's own open transition cannot displace it. */
  async function showSubmenu(index: number) {
    cancelSubmenuClose();
    if (openSubmenu === index) return;
    submenuFlip = false;
    submenuShift = 0;
    openSubmenu = index;
    await tick();
    if (!submenuElement || openSubmenu !== index) return;
    let rect = submenuElement.getBoundingClientRect();
    if (rect.right > window.innerWidth - 8) {
      submenuFlip = true;
      await tick();
      if (!submenuElement || openSubmenu !== index) return;
      rect = submenuElement.getBoundingClientRect();
    }
    const overflowBottom = rect.bottom - (window.innerHeight - 8);
    if (overflowBottom > 0) {
      submenuShift = Math.min(overflowBottom, Math.max(0, rect.top - 8));
    }
  }

  function close() {
    open = false;
    closeSubmenu();
  }

  function activate(item: ContextMenuItem) {
    if (item.children) return;
    item.action?.();
    close();
  }

  function handleClickOutside(event: MouseEvent) {
    if (menuElement && !(event.target as HTMLElement).closest('.menu')) {
      close();
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      if (openSubmenu !== null) closeSubmenu();
      else close();
    }
  }

  onMount(() => {
    document.addEventListener('click', handleClickOutside);
    document.addEventListener('contextmenu', handleClickOutside);
    document.addEventListener('keydown', handleKeydown);
  });

  onDestroy(() => {
    cancelSubmenuClose();
    document.removeEventListener('click', handleClickOutside);
    document.removeEventListener('contextmenu', handleClickOutside);
    document.removeEventListener('keydown', handleKeydown);
  });

  // A freshly opened menu never carries over the previous target's submenu.
  $effect(() => {
    if (!open) closeSubmenu();
  });

  /** Places the menu at the cursor, then pulls it back inside the viewport.
   *  The menu has to be on screen before it can be measured, so this runs
   *  once the requested position has been rendered. */
  async function placeMenu(cursorX: number, cursorY: number) {
    adjustedX = cursorX;
    adjustedY = cursorY;
    await tick();
    if (!menuElement) return;
    const rect = menuElement.getBoundingClientRect();
    if (rect.right > window.innerWidth - 8) {
      adjustedX = Math.max(8, window.innerWidth - rect.width - 8);
    }
    if (rect.bottom > window.innerHeight - 8) {
      adjustedY = Math.max(8, window.innerHeight - rect.height - 8);
    }
  }

  let adjustedX = $state(0);
  let adjustedY = $state(0);
  $effect(() => {
    if (open) placeMenu(x, y);
  });
</script>

{#if open}
  <ul
    bind:this={menuElement}
    class="menu"
    class:menu-unclipped={hasSubmenu}
    style="top:{adjustedY}px;left:{adjustedX}px"
    transition:fly={{ y: -8, duration: 180, easing: cubicOut }}
    role="menu"
  >
    {#each items as item, index (index)}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <li
        role="none"
        class:has-submenu={!!item.children}
        onmouseenter={() => (item.children ? showSubmenu(index) : closeSubmenu())}
        onmouseleave={() => item.children && scheduleSubmenuClose()}
      >
        <button
          type="button"
          class="entry"
          class:entry-danger={item.danger}
          class:entry-active={openSubmenu === index}
          onclick={() => (item.children ? showSubmenu(index) : activate(item))}
          role="menuitem"
          aria-haspopup={item.children ? 'menu' : undefined}
          aria-expanded={item.children ? openSubmenu === index : undefined}
          tabindex="0"
        >
          {#if item.icon}
            <span class="entry-icon" aria-hidden="true">{item.icon}</span>
          {/if}
          <span class="entry-label">{item.label}</span>
          {#if item.children}
            <svg
              class="entry-chevron"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <path d="M9 6l6 6-6 6" />
            </svg>
          {/if}
        </button>
        {#if item.children && openSubmenu === index}
          <ul
            bind:this={submenuElement}
            class="menu submenu"
            class:submenu-flip={submenuFlip}
            style="top:calc({-submenuShift}px - var(--space-1))"
            role="menu"
          >
            {#each item.children as child, childIndex (childIndex)}
              <li role="none">
                <button
                  type="button"
                  class="entry"
                  class:entry-danger={child.danger}
                  onclick={() => activate(child)}
                  role="menuitem"
                  tabindex="0"
                >
                  {#if child.icon}
                    <span class="entry-icon" aria-hidden="true">{child.icon}</span>
                  {/if}
                  <span class="entry-label">{child.label}</span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .menu {
    position: fixed;
    background: var(--color-surface-elevated);
    border: 1px solid var(--color-surface-outline);
    padding: var(--space-1);
    z-index: var(--z-modal);
    list-style: none;
    margin: 0;
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-md);
    min-width: 200px;
    max-height: 70vh;
    overflow-y: auto;
  }

  .menu-unclipped {
    max-height: none;
    overflow: visible;
  }

  .has-submenu {
    position: relative;
  }

  /* Anchored to its parent item, so it follows the menu's open transition
     instead of being displaced by it. The gap is bridged by the close delay. */
  .submenu {
    position: absolute;
    left: calc(100% + var(--space-1));
    min-width: 160px;
  }

  .submenu-flip {
    left: auto;
    right: calc(100% + var(--space-1));
  }

  .entry {
    padding: var(--space-2) var(--space-3);
    cursor: pointer;
    white-space: nowrap;
    background: none;
    border: none;
    border-radius: var(--radius-xs);
    color: var(--color-on-surface);
    width: 100%;
    text-align: left;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-weight: 400;
    font-size: var(--text-md);
  }

  .entry:hover,
  .entry:focus-visible,
  .entry-active {
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
  }

  .entry-danger {
    color: var(--color-error);
  }

  .entry-danger:hover,
  .entry-danger:focus-visible {
    background: color-mix(in srgb, var(--color-error) 14%, transparent);
    color: var(--color-error);
  }

  .entry-icon {
    font-size: var(--text-md);
    flex-shrink: 0;
  }

  .entry-label {
    flex: 1;
  }

  .entry-chevron {
    width: 1rem;
    height: 1rem;
    flex-shrink: 0;
    color: var(--color-muted);
  }

  .entry:focus-visible {
    outline: none;
  }
</style>
