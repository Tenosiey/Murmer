/**
 * `tauri build` refuses to run when a Tauri crate and its npm package differ
 * in major or minor version. Dependabot bumps the two sides in separate pull
 * requests, and CI never runs `tauri build`, so the first place a mismatch
 * surfaced was the release workflow — after the tag was pushed. This runs the
 * same comparison on every push.
 */
import { describe, expect, it } from 'vitest';
import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const root = (relative: string) => fileURLToPath(new URL(`../${relative}`, import.meta.url));

/** `name -> version` for every tauri crate in the shell's lockfile. */
function lockedCrates(): Map<string, string> {
  const lock = readFileSync(root('src-tauri/Cargo.lock'), 'utf8');
  const crates = new Map<string, string>();
  for (const m of lock.matchAll(/name = "(tauri(?:-plugin-[\w-]+)?)"\nversion = "([^"]+)"/g)) {
    crates.set(m[1], m[2]);
  }
  return crates;
}

/** The npm package the CLI pairs with a crate. */
function npmNameFor(crate: string): string {
  return crate === 'tauri' ? '@tauri-apps/api' : `@tauri-apps/${crate.replace(/^tauri-/, '')}`;
}

const majorMinor = (version: string) => version.split('.').slice(0, 2).join('.');

describe('Tauri crate and npm package versions', () => {
  const pairs = [...lockedCrates()]
    .map(([crate, version]) => {
      const manifest = root(`node_modules/${npmNameFor(crate)}/package.json`);
      if (!existsSync(manifest)) return null;
      const npmVersion: string = JSON.parse(readFileSync(manifest, 'utf8')).version;
      return { crate, version, npm: npmNameFor(crate), npmVersion };
    })
    .filter((pair) => pair !== null);

  it('found pairs to compare at all', () => {
    // A lockfile reformat that breaks the regex must not pass vacuously.
    expect(pairs.map((p) => p.crate)).toContain('tauri');
    expect(pairs.length).toBeGreaterThan(3);
  });

  it('agree on major and minor, as tauri build requires', () => {
    const mismatched = pairs
      .filter((p) => majorMinor(p.version) !== majorMinor(p.npmVersion))
      .map((p) => `${p.crate} ${p.version} vs ${p.npm} ${p.npmVersion}`);
    expect(mismatched).toEqual([]);
  });
});
