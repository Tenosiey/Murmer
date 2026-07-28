/**
 * Guards the two tables the client duplicates from the server.
 *
 * `AGENTS.md` asks twice to "keep them in sync", and nothing enforced it: the
 * copies drift silently, and the symptom shows up far from the cause — a
 * permission whose bit means one thing to the client and another to the
 * server, or a file picker that offers an extension `/upload` rejects.
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
  UPLOAD_CATEGORIES,
  DEFAULT_UPLOAD_MAX_BYTES,
  MIN_UPLOAD_MAX_BYTES,
  MAX_UPLOAD_MAX_BYTES
} from '../src/lib/chat/constants';

function readServerSource(relative: string): string {
  return readFileSync(fileURLToPath(new URL(`../../murmer_server/src/${relative}`, import.meta.url)), 'utf8');
}

const permissionsRs = readServerSource('permissions.rs');
const uploadRs = readServerSource('upload.rs');

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
