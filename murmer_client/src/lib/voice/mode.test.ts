// The mesh/SFU switch fails silently: a call that never leaves the mesh, a
// switch that cuts everybody's audio, or two transports playing the same
// voice twice all look like an ordinary call in a test that only checks the
// happy path. The slot map is untrusted input that decides whose audio comes
// out under whose name.
import { describe, expect, it } from 'vitest';
import {
  INITIAL_TRANSPORT,
  parseSfuSlots,
  parseVoiceMode,
  stepTransport,
  type TransportEvent,
  type TransportState
} from './mode';

function run(events: TransportEvent[], state: TransportState = INITIAL_TRANSPORT) {
  const actions: string[] = [];
  for (const event of events) {
    const step = stepTransport(state, event);
    state = step.state;
    actions.push(...step.actions);
  }
  return { state, actions };
}

describe('stepTransport', () => {
  it('keeps the mesh live until the SFU connects, then closes it', () => {
    const opened = run([{ type: 'mode', mode: 'sfu' }]);
    expect(opened.actions).toEqual(['open-sfu']);
    expect(opened.state.live).toBe('mesh');

    const switched = run([{ type: 'sfu-connected' }], opened.state);
    expect(switched.actions).toEqual(['close-mesh']);
    expect(switched.state.live).toBe('sfu');
  });

  it('keeps the SFU live until the mesh is ready, then closes it', () => {
    const onSfu = run([{ type: 'mode', mode: 'sfu' }, { type: 'sfu-connected' }]).state;
    const building = run([{ type: 'mode', mode: 'mesh' }], onSfu);
    expect(building.actions).toEqual(['build-mesh']);
    expect(building.state.live).toBe('sfu');

    const back = run([{ type: 'mesh-ready' }], building.state);
    expect(back.actions).toEqual(['close-sfu']);
    expect(back.state).toEqual(INITIAL_TRANSPORT);
  });

  it('drops a half-opened SFU when the channel shrinks before it connects', () => {
    const { state, actions } = run([
      { type: 'mode', mode: 'sfu' },
      { type: 'mode', mode: 'mesh' },
      // A late `connected` from the connection that was just closed.
      { type: 'sfu-connected' }
    ]);
    expect(actions).toEqual(['open-sfu', 'close-sfu']);
    expect(state).toEqual(INITIAL_TRANSPORT);
  });

  it('drops a half-built mesh when the channel grows back before it is ready', () => {
    const { state, actions } = run([
      { type: 'mode', mode: 'sfu' },
      { type: 'sfu-connected' },
      { type: 'mode', mode: 'mesh' },
      { type: 'mode', mode: 'sfu' },
      // The give-up timer of the abandoned mesh firing anyway.
      { type: 'mesh-ready' }
    ]);
    expect(actions).toEqual(['open-sfu', 'close-mesh', 'build-mesh', 'close-mesh']);
    expect(state).toEqual({ target: 'sfu', live: 'sfu', sfuOpen: true });
  });

  it('ignores a repeated mode and a reconnect of a live SFU', () => {
    const onSfu = run([{ type: 'mode', mode: 'sfu' }, { type: 'sfu-connected' }]).state;
    expect(run([{ type: 'mode', mode: 'sfu' }, { type: 'sfu-connected' }], onSfu).actions).toEqual(
      []
    );
    expect(run([{ type: 'mode', mode: 'mesh' }]).actions).toEqual([]);
  });
});

describe('parseVoiceMode', () => {
  it('reads both frames that carry a mode', () => {
    expect(parseVoiceMode({ type: 'voice-mode', channelId: 3, mode: 'sfu', slots: 9 })).toEqual({
      channelId: 3,
      mode: 'sfu',
      slots: 9
    });
    expect(
      parseVoiceMode({ type: 'voice-permissions', channelId: 3, canSpeak: true, mode: 'mesh', slots: 0 })
    ).toEqual({ channelId: 3, mode: 'mesh', slots: 0 });
  });

  it('refuses an SFU mode it could not offer for', () => {
    // No slots means an offer that hears nobody; a huge count means
    // thousands of transceivers from one frame.
    expect(parseVoiceMode({ channelId: 3, mode: 'sfu', slots: 0 })).toBeNull();
    expect(parseVoiceMode({ channelId: 3, mode: 'sfu', slots: 100_000 })).toBeNull();
    expect(parseVoiceMode({ channelId: 3, mode: 'sfu', slots: 2.5 })).toBeNull();
  });

  it('refuses an unknown mode or channel', () => {
    expect(parseVoiceMode({ channelId: 3, mode: 'turn', slots: 9 })).toBeNull();
    expect(parseVoiceMode({ channelId: '3', mode: 'sfu', slots: 9 })).toBeNull();
    expect(parseVoiceMode({ channelId: 3 })).toBeNull();
  });
});

describe('parseSfuSlots', () => {
  const slot = (mid: string, user: string, kind = 'audio') => ({ mid, user, kind });

  it('reads a well-formed slot map', () => {
    expect(
      parseSfuSlots({ channelId: 3, slots: [slot('2', 'bob'), slot('3', 'bob', 'video')] })
    ).toEqual({
      channelId: 3,
      slots: [
        { mid: '2', user: 'bob', kind: 'audio' },
        { mid: '3', user: 'bob', kind: 'video' }
      ]
    });
  });

  it('rejects the whole frame for one bad entry', () => {
    const good = slot('2', 'bob');
    for (const bad of [
      slot('2', 'carol'), // the same transceiver claimed twice
      slot('', 'carol'),
      slot('4', ''),
      slot('4', 'carol', 'screen'),
      { mid: 4, user: 'carol', kind: 'audio' },
      null,
      'bob'
    ]) {
      expect(parseSfuSlots({ channelId: 3, slots: [good, bad] })).toBeNull();
    }
  });

  it('rejects a frame without a channel or a list', () => {
    expect(parseSfuSlots({ slots: [] })).toBeNull();
    expect(parseSfuSlots({ channelId: 3, slots: {} })).toBeNull();
  });
});
