/**
 * Choosing where a forwarded message goes, and composing the one kind of
 * forward the server cannot make itself.
 *
 * Forwarding into a channel is a **server-side copy**: the client sends a
 * message id and a destination and nothing else, and the server rebuilds both
 * the words and the attribution from the row it stored
 * (`murmer_server/src/ws/helpers.rs`). Nothing in this file takes part in
 * that, which is the point — a forward puts words in somebody else's mouth in
 * front of a whole channel, and a client-composed one would be a forgery the
 * readers had no way to check.
 *
 * A direct message has no such path. Its content is end-to-end encrypted, so
 * the server can neither read the message being forwarded nor seal the copy,
 * and the attribution has to travel inside the ciphertext — as text, because
 * a DM's plaintext *is* its text. A forwarded DM is therefore a claim by its
 * sender rather than something the server vouches for, and that is the honest
 * shape for it: in a two-person conversation the sender could type the same
 * words anyway, so there is nothing a stamp would protect.
 */
import type { DialogOption } from '../stores/dialogs';
import type { ChannelInfo, Message } from '../types';

/** Where a forward is headed. */
export type ForwardTarget =
  | { kind: 'channel'; channelId: number }
  | { kind: 'dm'; user: string };

/** Encode a target as a dialog option value. */
export function forwardTargetValue(target: ForwardTarget): string {
  return target.kind === 'channel' ? `channel:${target.channelId}` : `dm:${target.user}`;
}

/** Decode a dialog option value, or null when it names nothing usable. */
export function parseForwardTarget(value: string | null): ForwardTarget | null {
  if (!value) return null;
  const separator = value.indexOf(':');
  if (separator < 0) return null;
  const kind = value.slice(0, separator);
  // Account names are the lookup key everywhere and may themselves contain a
  // colon, so only the first one separates the kind from the rest.
  const rest = value.slice(separator + 1);
  if (kind === 'channel') {
    // `Number('')` is 0, which is a plausible-looking id, so the empty case
    // has to be ruled out before the conversion rather than after it.
    const channelId = rest === '' ? Number.NaN : Number(rest);
    return Number.isInteger(channelId) && channelId > 0 ? { kind: 'channel', channelId } : null;
  }
  if (kind === 'dm') {
    return rest ? { kind: 'dm', user: rest } : null;
  }
  return null;
}

/**
 * The destinations offered when forwarding, channels first and then people.
 *
 * End-to-end encrypted channels are left out: the server holds no key to seal
 * a copy with, so it refuses those, and an option that can only fail is worse
 * than no option. The channel the message is already in is left out for the
 * same reason it would be pointless.
 *
 * Which channels a member may *write* to is not fully knowable here — a
 * per-channel override is only sent to managers — so every visible channel is
 * offered and the server has the final word, as it does for a typed message.
 */
export function forwardOptions(input: {
  channels: ChannelInfo[];
  currentChannelId: number;
  peers: string[];
  displayName: (user: string) => string;
}): DialogOption[] {
  const options: DialogOption[] = [];
  for (const channel of input.channels) {
    if (channel.e2ee || channel.id === input.currentChannelId) continue;
    options.push({
      value: forwardTargetValue({ kind: 'channel', channelId: channel.id }),
      label: `#${channel.name}`,
      description: channel.private ? 'Private channel' : undefined
    });
  }
  for (const peer of input.peers) {
    options.push({
      value: forwardTargetValue({ kind: 'dm', user: peer }),
      label: input.displayName(peer),
      description: 'Direct message'
    });
  }
  return options;
}

/** Markdown characters that would otherwise turn a name into formatting. */
const MARKDOWN_SPECIALS = /([\\`*_[\]])/g;

function escapeInline(value: string): string {
  return value.replace(MARKDOWN_SPECIALS, '\\$1');
}

/**
 * Compose the text of a message forwarded into a direct message.
 *
 * The attribution names the *original* author, not whoever last forwarded it —
 * the words are still the first author's, and a chain of "forwarded from a
 * forward of…" tells the reader nothing. It matches what the server stamps on
 * a channel forward for the same reason.
 *
 * A DM carries text and nothing else, so an image or attachment travels as its
 * link. The bytes are already on the server and the recipient can open them;
 * what would be lost silently is the fact that there *was* one.
 */
export function forwardedDmText(
  msg: Message,
  sourceChannel: string | null,
  displayName: (user: string) => string
): string {
  const origin = msg.forwardedFrom;
  const author = origin?.user ?? msg.user ?? msg.from ?? '';
  const channel = origin?.channel ?? sourceChannel ?? '';
  const who = author ? `**${escapeInline(displayName(author))}**` : 'someone';
  const where = channel ? ` in **#${escapeInline(channel)}**` : '';

  const parts = [`↪ Forwarded from ${who}${where}`];
  const text = msg.text?.trim();
  if (text) parts.push(text);
  if (typeof msg.image === 'string' && msg.image) {
    parts.push(msg.image);
  }
  if (msg.attachment) {
    parts.push(`[${escapeInline(msg.attachment.name)}](${msg.attachment.url})`);
  }
  return parts.join('\n\n');
}
