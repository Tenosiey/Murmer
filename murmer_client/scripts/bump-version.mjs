#!/usr/bin/env node
/*
  Computes the next release version and writes it into package.json,
  src-tauri/tauri.conf.json and src-tauri/Cargo.toml.

  Scheme: YYYY.MDD.N (year, month+day, counter for multiple releases on the
  same day), e.g. 2026.710.0 for the first release on 2026-07-10. The Tauri
  updater only offers an update when the new version is semver-greater than
  the installed one, which this scheme guarantees (a random suffix would not).

  Usage: npm run bump
*/
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const pkgPath = path.join(root, 'package.json');
const confPath = path.join(root, 'src-tauri', 'tauri.conf.json');
const cargoPath = path.join(root, 'src-tauri', 'Cargo.toml');

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

const conf = readFileSync(confPath, 'utf8');
writeFileSync(confPath, conf.replace(/"version":\s*"[^"]+"/, `"version": "${version}"`));

const cargo = readFileSync(cargoPath, 'utf8');
writeFileSync(cargoPath, cargo.replace(/^version\s*=\s*"[^"]+"/m, `version = "${version}"`));

console.log(`Bumped version: ${current} -> ${version}`);
console.log('Publish with:');
console.log(`  git commit -am "Release v${version}"`);
console.log(`  git tag v${version}`);
console.log(`  git push origin v${version}`);
