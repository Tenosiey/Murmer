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

  for (const key of Object.keys(env)) {
    if (key === 'SNAP' || key.startsWith('SNAP_')) {
      delete env[key];
    }
  }
}

const child = spawn('tauri', args, {
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
