#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';

const args = process.argv.slice(2);
const env = { ...process.env };

if (process.platform === 'linux') {
  const originalLdLibraryPath = env.LD_LIBRARY_PATH;
  if (originalLdLibraryPath) {
    const cleanedEntries = originalLdLibraryPath
      .split(':')
      .filter((entry) => entry && !/\/snap\/core\d+/.test(entry));

    if (cleanedEntries.length === 0) {
      delete env.LD_LIBRARY_PATH;
    } else {
      env.LD_LIBRARY_PATH = cleanedEntries.join(':');
    }
  }
}

// Invoke the Tauri CLI's JS entry point with the current Node binary instead
// of going through `npx` + a shell: it avoids shell quoting issues (DEP0190)
// and behaves the same on Linux, macOS and Windows.
const require = createRequire(import.meta.url);
const tauriBin = require.resolve('@tauri-apps/cli/tauri.js');

const child = spawn(process.execPath, [tauriBin, ...args], {
  env,
  stdio: 'inherit'
});

child.on('exit', (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }

  process.exit(code ?? 0);
});

child.on('error', (error) => {
  console.error('[murmer] Failed to launch Tauri CLI:', error);
  process.exit(1);
});
