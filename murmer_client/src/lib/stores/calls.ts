import { get, writable } from 'svelte/store';
import { chat } from './chat';
import { connection } from './connection';
import { session } from './session';
import { isBlocked } from './blocks';
import { displayNames } from './profiles';
import { notify } from '../notify';
import { startRingtone, stopRingtone } from '../voice/ringtone';
import { t } from '../i18n';
import type { Message } from '../types';

/**
 * Direct calls from a DM (`ws/handlers/calls.rs` on the server). The server
 * rings, and hands out a call id that is also the id of a voice channel only
 * the two parties can join, so once a call is answered it is an ordinary
 * voice session: the chat page joins and leaves it through the hooks below,
 * exactly as it would a channel.
 *
 * Only the device that placed or answered a call joins it. The account's
 * other devices still hear every frame, which is what lets them stop
 * ringing the moment one of them answers.
 */
export interface Call {
  id: number;
  /** The other party's account name. */
  peer: string;
  outgoing: boolean;
  accepted: boolean;
}

export const call = writable<Call | null>(null);

interface VoiceHooks {
  join: (id: number) => void;
  leave: (id: number) => void;
}
let hooks: VoiceHooks | null = null;
/** Whom this device asked to call, until the server's `call-ringing`. */
let dialling: string | null = null;
/** Whether this device placed or answered the current call, and so is in it. */
let joinedHere = false;

/** The chat page owns the voice session; it tells us how to drive it. */
export function setCallVoiceHooks(next: VoiceHooks | null) {
  hooks = next;
}

/** A `call-ringing` frame, checked; `null` when anything is off. */
export function parseRinging(
  msg: Message
): { id: number; caller: string; callee: string } | null {
  const { callId, caller, callee } = msg as Record<string, unknown>;
  if (!Number.isInteger(callId) || (callId as number) >= 0) return null;
  if (typeof caller !== 'string' || typeof callee !== 'string' || !caller || !callee) return null;
  return { id: callId as number, caller, callee };
}

function current(msg: Message): Call | null {
  const c = get(call);
  return c && (msg as { callId?: unknown }).callId === c.id ? c : null;
}

function reset() {
  stopRingtone();
  call.set(null);
  joinedHere = false;
}

chat.on('call-ringing', (msg: Message) => {
  const ringing = parseRinging(msg);
  const me = get(session).user;
  if (!ringing || !me || get(call)) return;
  if (ringing.callee === me) {
    // Unanswered, a blocked caller's call rings out on their side as
    // "missed"; the server never learns who is blocked.
    if (isBlocked(ringing.caller)) return;
    call.set({ id: ringing.id, peer: ringing.caller, outgoing: false, accepted: false });
    startRingtone();
    const name = get(displayNames)(ringing.caller);
    void notify(t('calls.incomingTitle'), t('calls.incoming', { name }));
  } else if (ringing.caller === me) {
    call.set({ id: ringing.id, peer: ringing.callee, outgoing: true, accepted: false });
    if (dialling === ringing.callee) {
      dialling = null;
      joinedHere = true;
      startRingtone();
      hooks?.join(ringing.id);
    }
  }
});

chat.on('call-accepted', (msg: Message) => {
  const c = current(msg);
  if (!c) return;
  stopRingtone();
  // Answered on another of our devices: this one is not in the call.
  if (!c.outgoing && !joinedHere) {
    call.set(null);
    return;
  }
  call.set({ ...c, accepted: true });
});

chat.on('call-ended', (msg: Message) => {
  const c = current(msg);
  if (!c) return;
  if (joinedHere) hooks?.leave(c.id);
  reset();
});

// The server ends a call with the connection that carried it.
connection.subscribe((state) => {
  if (state !== 'connected') {
    dialling = null;
    reset();
  }
});

/** Ring `user`. A refusal comes back as an error frame (busy, offline). */
export function startCall(user: string) {
  if (get(call)) return;
  dialling = user;
  chat.sendRaw({ type: 'call-start', target: user });
}

/** Answer the call ringing on this device and join it. */
export function acceptCall() {
  const c = get(call);
  if (!c || c.outgoing || c.accepted) return;
  joinedHere = true;
  stopRingtone();
  chat.sendRaw({ type: 'call-accept', callId: c.id });
  call.set({ ...c, accepted: true });
  hooks?.join(c.id);
}

/** Decline, cancel or hang up; the server's `call-ended` does the rest. */
export function endCall() {
  const c = get(call);
  if (c) chat.sendRaw({ type: 'call-end', callId: c.id });
}
