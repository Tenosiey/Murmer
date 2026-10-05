/**
 * Svelte action for a modal's content element: take focus when it mounts,
 * give it back to whatever held it when it unmounts.
 *
 * The modals hear Escape on their content, so the content has to hold focus
 * from the moment it opens. Opened by a click — a header button, a context
 * menu entry — focus otherwise stays on whatever was clicked, and Escape did
 * nothing until the user clicked into the modal. Handing focus back on close
 * keeps a keyboard user where they were instead of at the top of the page.
 *
 * Meant for content rendered inside `{#if open}`, so mounting is opening.
 */
export function modalFocus(node: HTMLElement) {
  const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  node.focus();
  return {
    destroy() {
      previous?.focus();
    }
  };
}
