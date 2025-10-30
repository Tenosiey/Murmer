#!/usr/bin/env node
import { spawn } from 'node:child_process';

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

const child = spawn('npx', ['tauri', ...args], {
  env,
  stdio: 'inherit',
  shell: true
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
