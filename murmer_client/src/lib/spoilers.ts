/**
 * Spoilers: `||text||` in a message and images sent with the spoiler toggle
 * render inside a `.spoiler` element that stays hidden until it is clicked.
 *
 * One listener on the document reveals them all, so every surface that shows
 * a message — the channel, threads, DMs, pins — gets it without wiring its
 * own. Revealing is per element and lasts until the content re-renders;
 * nothing is stored.
 */

/** `||text||` written by the user, for previews that cannot hide it. */
const SPOILER_MARKUP = /\|\|(?=\S)[\s\S]*?\S\|\|/g;

/** Text with every spoiler blanked out, for notifications and other plain previews. */
export function hideSpoilers(text: string): string {
  return text.replace(SPOILER_MARKUP, '[spoiler]');
}

function revealTarget(event: Event): HTMLElement | null {
  const target = event.target as Element | null;
  const spoiler = target?.closest?.('.spoiler:not(.revealed)');
  return spoiler instanceof HTMLElement ? spoiler : null;
}

function reveal(spoiler: HTMLElement, event: Event) {
  // The first activation only reveals: a link or image inside a hidden
  // spoiler must not be followed sight unseen.
  event.preventDefault();
  event.stopPropagation();
  spoiler.classList.add('revealed');
  spoiler.removeAttribute('role');
  spoiler.removeAttribute('tabindex');
  spoiler.removeAttribute('aria-label');
}

/** Install the document-wide reveal handler; returns its removal. */
export function installSpoilerReveal(): () => void {
  const onClick = (event: MouseEvent) => {
    const spoiler = revealTarget(event);
    if (spoiler) reveal(spoiler, event);
  };
  const onKeydown = (event: KeyboardEvent) => {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    const spoiler = revealTarget(event);
    if (spoiler) reveal(spoiler, event);
  };
  // Capture, so the reveal runs before a link's own click handling.
  document.addEventListener('click', onClick, true);
  document.addEventListener('keydown', onKeydown, true);
  return () => {
    document.removeEventListener('click', onClick, true);
    document.removeEventListener('keydown', onKeydown, true);
  };
}
