/**
 * Guards the tables the client duplicates from the server.
 *
 * The documentation asked to "keep them in sync" and nothing enforced it: the
 * copies drift silently, and the symptom shows up far from the cause — a
 * permission whose bit means one thing to the client and another to the
 * server, or a file picker that offers an extension `/upload` rejects.
 * `agents/skills/mirrored-constants.md` is the procedure for changing one.
 *
 * These read the Rust sources as text rather than executing them, so they stay
 * a plain Vitest run with no toolchain of their own. That means they are
 * coupled to the *formatting* of those files: if a rewrite makes a regex stop
 * matching, the parse assertions below fail loudly instead of silently
 * comparing nothing.
 */
import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { PERMISSIONS, ALL_PERMISSIONS } from '../src/lib/chat/permissions';
import {
  AUDIT_ACTIONS,
  AUDIT_ACTOR_ADMIN_TOKEN,
  auditTargetIsMember
} from '../src/lib/chat/audit';
import {
  UPLOAD_CATEGORIES,
  DEFAULT_UPLOAD_MAX_BYTES,
  MIN_UPLOAD_MAX_BYTES,
  MAX_UPLOAD_MAX_BYTES,
  MAX_MESSAGE_LENGTH,
  MIN_CONFIGURABLE_MESSAGE_LENGTH,
  MAX_SLOW_MODE_SECONDS,
  MAX_PROFANITY_WORDS,
  MAX_PROFANITY_WORD_LEN,
  MAX_VOICE_BITRATE,
  MIN_MUTE_SECONDS,
  MAX_MUTE_SECONDS,
  MAX_AUTOMOD_RULES,
  MAX_AUTOMOD_PATTERN_LEN,
  MAX_AUTOMOD_NAME_LEN,
  DEFAULT_AUTOMOD_MUTE_SECONDS,
  AUTOMOD_KINDS,
  AUTOMOD_ACTIONS,
  MIN_SOUND_NAME_LEN,
  MAX_SOUND_NAME_LEN,
  MAX_SOUND_FILE_BYTES,
  MAX_SOUNDBOARD_SOUNDS,
  SOUND_EXTENSIONS,
  SOUNDBOARD_COOLDOWN_MS
} from '../src/lib/chat/constants';

function readServerSource(relative: string): string {
  return readFileSync(fileURLToPath(new URL(`../../murmer_server/src/${relative}`, import.meta.url)), 'utf8');
}

const permissionsRs = readServerSource('permissions.rs');
const uploadRs = readServerSource('upload.rs');
const chatSettingsRs = readServerSource('db/chat_settings.rs');
const voiceDefaultsRs = readServerSource('db/voice_defaults.rs');
const moderationRs = readServerSource('db/moderation.rs');
const automodRs = readServerSource('automod.rs');
const auditRs = readServerSource('db/audit.rs');
const wsConstantsRs = readServerSource('ws/constants.rs');

/** `pub const NAME: Permissions = 1 << N;` — one line per flag. */
function serverPermissionFlags(): Record<string, number> {
  const flags: Record<string, number> = {};
  for (const match of permissionsRs.matchAll(
    /^pub const ([A-Z_]+): Permissions = 1 << (\d+);$/gm
  )) {
    flags[match[1]] = 1 << Number(match[2]);
  }
  return flags;
}

/** Evaluate a `10 * 1024 * 1024`-style literal without running Rust. */
function serverByteConstant(name: string): number {
  const match = uploadRs.match(new RegExp(`^pub const ${name}: usize = ([0-9 *]+);$`, 'm'));
  expect(match, `${name} not found in upload.rs`).not.toBeNull();
  return match![1]
    .split('*')
    .map((part) => Number(part.trim()))
    .reduce((product, value) => product * value, 1);
}

/** Evaluate a `6 * 60 * 60`-style numeric literal from any server source. */
function serverNumberConstant(source: string, name: string, type: string): number {
  const match = source.match(new RegExp(`^pub const ${name}: ${type} = ([0-9_ *]+);$`, 'm'));
  expect(match, `${name} not found`).not.toBeNull();
  return match![1]
    .split('*')
    .map((part) => Number(part.trim().replace(/_/g, '')))
    .reduce((product, value) => product * value, 1);
}

/**
 * Wire names from an enum's `as_str` block: `Self::Word => "word",`. The
 * server rejects a kind or action it does not know rather than falling back
 * to a default, so a client offering one it spelled differently would save
 * nothing and say nothing.
 */
function serverWireNames(source: string, enumName: string): string[] {
  const block = source.match(new RegExp(`^impl ${enumName} \\{([\\s\\S]*?)\\n\\}`, 'm'));
  expect(block, `impl ${enumName} not found in automod.rs`).not.toBeNull();
  return [...block![1].matchAll(/Self::\w+ => "([a-z]+)"/g)].map((match) => match[1]);
}

/** `pub const NAME: &[&str] = &["a", "b"];` — a safe-list as written. */
function serverStrList(source: string, name: string): string[] {
  const match = source.match(
    new RegExp(String.raw`^pub const ${name}: &\[&str\] = &\[([^\]]*)\];$`, 'm')
  );
  expect(match, `${name} not found`).not.toBeNull();
  return [...match![1].matchAll(/"([^"]+)"/g)].map((m) => m[1]);
}

type ServerCategory = { id: string; label: string; extensions: string[] };

function serverUploadCategories(): ServerCategory[] {
  const block = uploadRs.match(
    /pub static UPLOAD_CATEGORIES: &\[UploadCategory\] = &\[([\s\S]*?)\n\];/
  );
  expect(block, 'UPLOAD_CATEGORIES not found in upload.rs').not.toBeNull();

  const categories: ServerCategory[] = [];
  for (const match of block![1].matchAll(
    /UploadCategory\s*\{\s*id:\s*"([^"]+)",\s*label:\s*"([^"]+)",\s*extensions:\s*&\[([^\]]*)\],\s*\}/g
  )) {
    categories.push({
      id: match[1],
      label: match[2],
      extensions: [...match[3].matchAll(/"([^"]+)"/g)].map((e) => e[1])
    });
  }
  return categories;
}

describe('permission bitmask mirror', () => {
  const server = serverPermissionFlags();

  it('parsed the server flags at all', () => {
    // Cheap canary: a reformat that breaks the regex must not turn every
    // assertion below into a comparison of two empty objects.
    expect(Object.keys(server).length).toBeGreaterThan(10);
  });

  it('defines the same flag names on both sides', () => {
    expect(Object.keys(server).sort()).toEqual(Object.keys(PERMISSIONS).sort());
  });

  it('gives every flag the same bit on both sides', () => {
    expect(server).toEqual({ ...PERMISSIONS });
  });

  it('lists every flag in the server-side ALL union', () => {
    // `ALL` is written out by hand in Rust, so a new flag can be added to the
    // file and forgotten here — which would let the server reject it as an
    // unknown bit coming from a client that does know about it.
    const all = permissionsRs.match(/pub const ALL: Permissions = ([\s\S]*?);/);
    expect(all).not.toBeNull();
    const named = [...all![1].matchAll(/[A-Z_]+/g)].map((m) => m[0]);
    expect(named.sort()).toEqual(Object.keys(server).sort());
  });

  it('agrees with the client union', () => {
    const union = Object.values(server).reduce((acc, bit) => acc | bit, 0);
    expect(union).toBe(ALL_PERMISSIONS);
  });
});

describe('upload safe-list mirror', () => {
  const server = serverUploadCategories();

  it('parsed the server categories at all', () => {
    expect(server.length).toBeGreaterThan(1);
    for (const category of server) {
      expect(category.extensions.length).toBeGreaterThan(0);
    }
  });

  it('lists the same categories in the same order', () => {
    expect(server.map((c) => c.id)).toEqual(UPLOAD_CATEGORIES.map((c) => c.id));
    expect(server.map((c) => c.label)).toEqual(UPLOAD_CATEGORIES.map((c) => c.label));
  });

  it('lists the same extensions per category', () => {
    expect(server).toEqual(
      UPLOAD_CATEGORIES.map((c) => ({ id: c.id, label: c.label, extensions: c.extensions }))
    );
  });

  it('never admits anything a browser might execute', () => {
    // The server refuses to serve active content from `/files`; an extension
    // slipping into either copy is the bug this catches.
    const active = [
      'html',
      'htm',
      'xhtml',
      'shtml',
      'svg',
      'svgz',
      'xml',
      'js',
      'mjs',
      'cjs',
      'css',
      'php',
      'jsp',
      'asp',
      'aspx',
      'exe',
      'bat',
      'cmd',
      'sh',
      'ps1',
      'jar',
      'wasm'
    ];
    const listed = new Set([
      ...server.flatMap((c) => c.extensions),
      ...UPLOAD_CATEGORIES.flatMap((c) => c.extensions)
    ]);
    for (const extension of active) {
      expect(listed.has(extension), `"${extension}" is on the upload safe-list`).toBe(false);
    }
  });

  it('has no extension in two categories', () => {
    const all = server.flatMap((c) => c.extensions);
    expect(new Set(all).size).toBe(all.length);
  });

  it('agrees on the configurable size bounds', () => {
    expect(serverByteConstant('DEFAULT_MAX_FILE_SIZE')).toBe(DEFAULT_UPLOAD_MAX_BYTES);
    expect(serverByteConstant('MIN_CONFIGURABLE_FILE_SIZE')).toBe(MIN_UPLOAD_MAX_BYTES);
    expect(serverByteConstant('MAX_CONFIGURABLE_FILE_SIZE')).toBe(MAX_UPLOAD_MAX_BYTES);
  });
});

describe('soundboard mirror', () => {
  // A sound is stored by `/upload` first and only then registered over the
  // WebSocket, so every drift here is paid after the file is already on disk:
  // the client offers a file the `add-sound` frame rejects, leaving an orphan
  // upload and an error the user cannot act on.
  const extensions = serverStrList(wsConstantsRs, 'UPLOAD_SOUND_EXTENSIONS');

  it('parsed the server extensions at all', () => {
    expect(extensions.length).toBeGreaterThan(1);
  });

  it('offers exactly the extensions `add-sound` accepts', () => {
    expect(extensions).toEqual(SOUND_EXTENSIONS);
  });

  it('keeps the sound extensions inside the upload audio category', () => {
    // `/upload` gates on its own category safe-list, so an extension only
    // `add-sound` knows about is rejected before it is ever stored.
    const audio = serverUploadCategories().find((c) => c.id === 'audio');
    expect(audio, 'no audio category in upload.rs').toBeDefined();
    for (const extension of extensions) {
      expect(audio!.extensions, `"${extension}" is not an audio upload`).toContain(extension);
    }
  });

  it('agrees on the file size and sound count limits', () => {
    expect(serverNumberConstant(wsConstantsRs, 'MAX_SOUND_FILE_BYTES', 'u64')).toBe(
      MAX_SOUND_FILE_BYTES
    );
    expect(serverNumberConstant(wsConstantsRs, 'MAX_SOUNDBOARD_SOUNDS', 'i64')).toBe(
      MAX_SOUNDBOARD_SOUNDS
    );
  });

  it('agrees on the name length bounds', () => {
    expect(serverNumberConstant(wsConstantsRs, 'MIN_SOUND_NAME_LEN', 'usize')).toBe(
      MIN_SOUND_NAME_LEN
    );
    expect(serverNumberConstant(wsConstantsRs, 'MAX_SOUND_NAME_LEN', 'usize')).toBe(
      MAX_SOUND_NAME_LEN
    );
  });

  it('agrees on the playback cooldown', () => {
    // Cosmetic on the client — the server enforces it — but a client that
    // thinks the window is shorter re-enables the button into a rejection.
    expect(serverNumberConstant(wsConstantsRs, 'SOUNDBOARD_COOLDOWN_MS', 'u64')).toBe(
      SOUNDBOARD_COOLDOWN_MS
    );
  });
});

describe('chat policy mirror', () => {
  // The server clamps everything it is sent, so a drift here does not let a
  // client widen a limit — it makes the dashboard offer a value the server
  // will silently change, which is worse to debug than a rejection.
  it('agrees on the message length bounds', () => {
    expect(serverNumberConstant(chatSettingsRs, 'MAX_MESSAGE_LENGTH', 'usize')).toBe(
      MAX_MESSAGE_LENGTH
    );
    expect(
      serverNumberConstant(chatSettingsRs, 'MIN_CONFIGURABLE_MESSAGE_LENGTH', 'usize')
    ).toBe(MIN_CONFIGURABLE_MESSAGE_LENGTH);
  });

  it('agrees on the slow mode ceiling', () => {
    expect(serverNumberConstant(chatSettingsRs, 'MAX_SLOW_MODE_SECONDS', 'u64')).toBe(
      MAX_SLOW_MODE_SECONDS
    );
  });

  it('agrees on the profanity list bounds', () => {
    expect(serverNumberConstant(chatSettingsRs, 'MAX_PROFANITY_WORDS', 'usize')).toBe(
      MAX_PROFANITY_WORDS
    );
    expect(serverNumberConstant(chatSettingsRs, 'MAX_PROFANITY_WORD_LEN', 'usize')).toBe(
      MAX_PROFANITY_WORD_LEN
    );
  });

  it('agrees on the voice bitrate ceiling', () => {
    expect(serverNumberConstant(voiceDefaultsRs, 'MAX_ALLOWED_VOICE_BITRATE', 'i32')).toBe(
      MAX_VOICE_BITRATE
    );
  });
});

describe('auto-moderation mirror', () => {
  it('parsed the server vocabularies at all', () => {
    // Cheap canary, as above: a reformat that breaks the regex must not turn
    // the comparisons below into two empty arrays agreeing with each other.
    expect(serverWireNames(automodRs, 'RuleKind').length).toBe(3);
    expect(serverWireNames(automodRs, 'RuleAction').length).toBe(3);
  });

  it('offers exactly the match kinds the server accepts', () => {
    expect(serverWireNames(automodRs, 'RuleKind')).toEqual(AUTOMOD_KINDS.map((kind) => kind.id));
  });

  it('offers the actions in the server\'s severity order', () => {
    // The Rust enum is ordered least to most severe and compared as such —
    // that ordering is what decides between two matching rules, so the
    // dashboard listing them in another order would misdescribe the outcome.
    expect(serverWireNames(automodRs, 'RuleAction')).toEqual(
      AUTOMOD_ACTIONS.map((action) => action.id)
    );
  });

  it('agrees on the rule bounds', () => {
    expect(serverNumberConstant(automodRs, 'MAX_AUTOMOD_RULES', 'usize')).toBe(MAX_AUTOMOD_RULES);
    expect(serverNumberConstant(automodRs, 'MAX_AUTOMOD_PATTERN_LEN', 'usize')).toBe(
      MAX_AUTOMOD_PATTERN_LEN
    );
    expect(serverNumberConstant(automodRs, 'MAX_AUTOMOD_NAME_LEN', 'usize')).toBe(
      MAX_AUTOMOD_NAME_LEN
    );
    expect(serverNumberConstant(automodRs, 'DEFAULT_AUTOMOD_MUTE_SECONDS', 'i64')).toBe(
      DEFAULT_AUTOMOD_MUTE_SECONDS
    );
  });

  it('agrees on the mute duration bounds a rule is clamped to', () => {
    expect(serverNumberConstant(moderationRs, 'MIN_MUTE_SECONDS', 'i64')).toBe(MIN_MUTE_SECONDS);
    expect(serverNumberConstant(moderationRs, 'MAX_MUTE_SECONDS', 'i64')).toBe(MAX_MUTE_SECONDS);
  });
});

describe('audit action mirror', () => {
  /** The `pub const NAME: &str = "wire-name";` lines inside `mod actions`. */
  function serverAuditActions(): string[] {
    const block = auditRs.match(/pub mod actions \{([\s\S]*?)\n\}/);
    expect(block).not.toBeNull();
    return [...block![1].matchAll(/pub const [A-Z_]+: &str = "([^"]+)";/g)].map((m) => m[1]);
  }

  it('parsed the server actions at all', () => {
    // Cheap canary, as above: a reformat that breaks the regex must not turn
    // the coverage assertion into an empty list nothing can be missing from.
    expect(serverAuditActions().length).toBeGreaterThan(10);
  });

  it('labels every action the server can record', () => {
    // An unlabelled action still renders — `auditActionLabel` falls back to
    // the wire name — so this is the difference between a readable log and a
    // log of raw identifiers, not between a log and a crash.
    expect(serverAuditActions().sort()).toEqual(Object.keys(AUDIT_ACTIONS).sort());
  });

  it('only claims a member target for actions the server records', () => {
    // The set that decides this is a third copy of the action names, and a
    // stale entry there would run a role or channel name through the member
    // nickname lookup — relabelling it as whoever shares the name.
    const known = serverAuditActions();
    for (const action of known) expect(typeof auditTargetIsMember(action)).toBe('boolean');
    expect(known.filter(auditTargetIsMember).sort()).toEqual(
      ['admin-role-grant', 'ban', 'kick', 'mute', 'unban', 'unmute', 'user-roles'].sort()
    );
  });

  it('agrees on the sentinel the /role endpoint is recorded under', () => {
    // It has to be a name no account can hold, which is what the parentheses
    // buy: `validate_user_name` allows only alphanumerics, dashes,
    // underscores and spaces.
    const actor = auditRs.match(/pub const ACTOR_ADMIN_TOKEN: &str = "([^"]+)";/);
    expect(actor).not.toBeNull();
    expect(actor![1]).toBe(AUDIT_ACTOR_ADMIN_TOKEN);
    expect(AUDIT_ACTOR_ADMIN_TOKEN).not.toMatch(/^[A-Za-z0-9_ -]+$/);
  });
});
