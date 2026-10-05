/**
 * Svelte action that makes an element a drop target for files.
 *
 * Only drags carrying files are taken: dragging text or a link across the
 * chat must keep its default behaviour. Enter and leave fire for every child
 * the pointer crosses, so whether a drag is "over" the element is a depth
 * count rather than the last event seen — otherwise the drop hint flickers
 * off the moment the pointer passes over a message.
 */

export type FileDropOptions = {
  /** The first dropped file; the chat sends one attachment at a time. */
  onFile: (file: File) => void;
  /** Whether a file drag is currently over the element. */
  onActiveChange: (active: boolean) => void;
};

function hasFiles(event: DragEvent): boolean {
  return Array.from(event.dataTransfer?.types ?? []).includes('Files');
}

export function fileDrop(node: HTMLElement, options: FileDropOptions) {
  let opts = options;
  let depth = 0;

  function setDepth(next: number) {
    const wasActive = depth > 0;
    depth = next;
    if (wasActive !== depth > 0) opts.onActiveChange(depth > 0);
  }

  function enter(event: DragEvent) {
    if (!hasFiles(event)) return;
    event.preventDefault();
    setDepth(depth + 1);
  }

  function over(event: DragEvent) {
    if (!hasFiles(event)) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy';
  }

  function leave(event: DragEvent) {
    if (!hasFiles(event)) return;
    setDepth(Math.max(0, depth - 1));
  }

  function drop(event: DragEvent) {
    if (!hasFiles(event)) return;
    event.preventDefault();
    setDepth(0);
    const file = Array.from(event.dataTransfer?.files ?? [])[0];
    if (file) opts.onFile(file);
  }

  node.addEventListener('dragenter', enter);
  node.addEventListener('dragover', over);
  node.addEventListener('dragleave', leave);
  node.addEventListener('drop', drop);

  return {
    update(next: FileDropOptions) {
      opts = next;
    },
    destroy() {
      node.removeEventListener('dragenter', enter);
      node.removeEventListener('dragover', over);
      node.removeEventListener('dragleave', leave);
      node.removeEventListener('drop', drop);
    }
  };
}
