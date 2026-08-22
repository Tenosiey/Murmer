/**
 * How much disk the server's uploads directory is using, per file category.
 *
 * Requested on demand by the Server Dashboard's Files & Uploads tab: the
 * server walks the directory when asked rather than tracking a counter,
 * because files also arrive through the emoji, avatar and soundboard flows
 * and a counter that drifts from the directory is worse than none. Only
 * managers get an answer, so a `null` value means "not measured yet", not
 * "empty".
 */
import { writable } from 'svelte/store';
import { chat } from './chat';
import { connection } from './connection';
import type { Message } from '../types';

export interface StorageCategoryUsage {
  /** Category id (`images`, `documents`, …, or `other`). */
  id: string;
  bytes: number;
  files: number;
}

export interface StorageUsage {
  totalBytes: number;
  fileCount: number;
  /** Per-category breakdown, largest first. */
  categories: StorageCategoryUsage[];
}

function parseCount(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? Math.round(value) : 0;
}

function createStorageUsageStore() {
  /** null until the server has answered on this connection. */
  const { subscribe, set } = writable<StorageUsage | null>(null);

  chat.on('storage-usage', (msg: Message) => {
    const payload = msg as any;
    const rawCategories =
      payload.categories && typeof payload.categories === 'object' ? payload.categories : {};
    const categories: StorageCategoryUsage[] = Object.entries(rawCategories)
      .map(([id, entry]) => ({
        id,
        bytes: parseCount((entry as any)?.bytes),
        files: parseCount((entry as any)?.files)
      }))
      .sort((a, b) => b.bytes - a.bytes);
    set({
      totalBytes: parseCount(payload.totalBytes),
      fileCount: parseCount(payload.fileCount),
      categories
    });
  });

  connection.subscribe((state) => {
    if (state !== 'connected') set(null);
  });

  /** Ask the server to measure the upload directory (managers only). */
  function refresh(): void {
    chat.sendRaw({ type: 'get-storage-usage' });
  }

  return { subscribe, refresh };
}

export const storageUsage = createStorageUsageStore();

/** Render a byte count for the dashboard: KB, MB or GB with one decimal. */
export function formatBytes(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} B`;
}
