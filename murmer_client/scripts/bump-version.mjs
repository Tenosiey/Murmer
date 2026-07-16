#!/usr/bin/env node
/*
  Computes the next release version and writes it into every versioned
  manifest of the monorepo:

  - murmer_client/package.json + package-lock.json
  - murmer_client/src-tauri/tauri.conf.json
  - murmer_client/src-tauri/Cargo.toml + Cargo.lock
  - murmer_server/Cargo.toml + Cargo.lock

  Client and server are bumped in lockstep so a release tag identifies one
  consistent state of the whole repository; bumping them separately proved
  easy to forget.

  Scheme: YYYY.MDD.N (year, month+day, counter for multiple releases on the
  same day), e.g. 2026.710.0 for the first release on 2026-07-10. The Tauri
  updater only offers an update when the new version is semver-greater than
  the installed one, which this scheme guarantees (a random suffix would not).

  Usage: npm run bump
*/
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const clientRoot = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const repoRoot = path.dirname(clientRoot);
const serverRoot = path.join(repoRoot, 'murmer_server');

const pkgPath = path.join(clientRoot, 'package.json');
const confPath = path.join(clientRoot, 'src-tauri', 'tauri.conf.json');

const now = new Date();
const datePart = `${now.getFullYear()}.${now.getMonth() + 1}${String(now.getDate()).padStart(2, '0')}`;

const pkg = JSON.parse(readFileSync(pkgPath, 'utf8'));
const current = pkg.version;

let counter = 0;
if (current.startsWith(`${datePart}.`)) {
  const previous = Number(current.slice(datePart.length + 1));
  counter = Number.isFinite(previous) ? previous + 1 : 0;
}
const version = `${datePart}.${counter}`;

pkg.version = version;
writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + '\n');

// package-lock.json records the package's own version twice (top level and
// the "" entry in packages); leaving it stale caused mismatch fixup commits.
const lockJsonPath = path.join(clientRoot, 'package-lock.json');
const lockJson = JSON.parse(readFileSync(lockJsonPath, 'utf8'));
lockJson.version = version;
if (lockJson.packages && lockJson.packages['']) {
  lockJson.packages[''].version = version;
}
writeFileSync(lockJsonPath, JSON.stringify(lockJson, null, 2) + '\n');

const conf = readFileSync(confPath, 'utf8');
writeFileSync(confPath, conf.replace(/"version":\s*"[^"]+"/, `"version": "${version}"`));

// Rewrites the `version` of the crate's own [package] section (the first
// `version =` line in Cargo.toml; dependency pins further down are untouched).
function bumpCargoToml(cargoPath) {
  const cargo = readFileSync(cargoPath, 'utf8');
  writeFileSync(cargoPath, cargo.replace(/^version\s*=\s*"[^"]+"/m, `version = "${version}"`));
}

// Cargo.lock pins the workspace crate's own version too; builds with
// `--locked` (Docker, CI) fail when it disagrees with Cargo.toml.
function bumpCargoLock(lockPath, crateName) {
  const lock = readFileSync(lockPath, 'utf8');
  // \r?\n: the working copy may have CRLF line endings (git autocrlf).
  const block = new RegExp(`(name = "${crateName}"\\r?\\nversion = )"[^"]+"`);
  if (!block.test(lock)) {
    throw new Error(`could not find crate ${crateName} in ${lockPath}`);
  }
  writeFileSync(lockPath, lock.replace(block, `$1"${version}"`));
}

bumpCargoToml(path.join(clientRoot, 'src-tauri', 'Cargo.toml'));
bumpCargoLock(path.join(clientRoot, 'src-tauri', 'Cargo.lock'), 'murmer_client');
bumpCargoToml(path.join(serverRoot, 'Cargo.toml'));
bumpCargoLock(path.join(serverRoot, 'Cargo.lock'), 'murmer_server');

console.log(`Bumped client and server: ${current} -> ${version}`);
console.log('Publish with:');
console.log(`  git commit -am "Release v${version}"`);
console.log(`  git tag v${version}`);
console.log(`  git push origin v${version}`);
