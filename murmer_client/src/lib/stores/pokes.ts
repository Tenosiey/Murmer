import { get } from 'svelte/store';
import { chat } from './chat';
import { dialogs } from './dialogs';
import { isBlocked } from './blocks';
import { displayNames } from './profiles';
import { notify } from '../notify';
import type { Message } from '../types';
import { t } from '../i18n';

/**
 * Pokes: a short nudge from one member to another (TeamSpeak). It pops up as
 * a dialog and an OS notification even when every channel is muted — that is
 * the point of it, and why the server paces it per sender. A poke from a
 * blocked member is dropped on arrival, like their DMs: the server never
 * learns who is blocked.
 */
chat.on('poke', (msg: Message) => {
  const from = typeof msg.from === 'string' ? msg.from : '';
  if (!from || isBlocked(from)) return;
  const name = get(displayNames)(from);
  void dialogs.alert({ title: t('poke.title'), message: t('poke.message', { name }) });
  void notify(t('poke.title'), t('poke.message', { name }));
});

/** Poke `user`. The server answers with an error frame when it refuses. */
export function poke(user: string) {
  chat.sendRaw({ type: 'poke', target: user });
}
