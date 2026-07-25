/**
 * Server-wide upload policy: the per-file size cap and which file categories
 * members may attach.
 *
 * The server announces the policy after authentication and broadcasts it on
 * change; it enforces the policy on `/upload` regardless of what this store
 * says. The client copy exists to reject a file before spending an upload on
 * it and to render the rules in the Server Dashboard.
 */
import { writable, derived, get } from 'svelte/store';
import { chat } from './chat';
import { connection } from './connection';
import {
  DEFAULT_UPLOAD_MAX_BYTES,
  UPLOAD_CATEGORIES
} from '../chat/constants';
import type { Message } from '../types';

export interface UploadPolicy {
  /** Largest accepted file size in bytes. */
  maxBytes: number;
  /** Ids of the enabled categories; an empty list disables uploads. */
  categories: string[];
}

const KNOWN_CATEGORY_IDS = UPLOAD_CATEGORIES.map((category) => category.id);

function defaultPolicy(): UploadPolicy {
  return { maxBytes: DEFAULT_UPLOAD_MAX_BYTES, categories: [...KNOWN_CATEGORY_IDS] };
}

export const uploadConfig = writable<UploadPolicy>(defaultPolicy());

chat.on('upload-config', (msg: Message) => {
  const rawMax = (msg as any).maxBytes;
  const rawCategories = (msg as any).categories;
  // Validate before mutating client state: a malformed frame must not widen
  // the categories the picker offers or the size it accepts.
  const maxBytes =
    typeof rawMax === 'number' && Number.isFinite(rawMax) && rawMax > 0
      ? Math.round(rawMax)
      : DEFAULT_UPLOAD_MAX_BYTES;
  const categories = Array.isArray(rawCategories)
    ? rawCategories.filter(
        (id: unknown): id is string => typeof id === 'string' && KNOWN_CATEGORY_IDS.includes(id)
      )
    : [...KNOWN_CATEGORY_IDS];
  uploadConfig.set({ maxBytes, categories });
});

connection.subscribe((state) => {
  // Forget the previous server's policy; the next one announces its own.
  if (state !== 'connected') uploadConfig.set(defaultPolicy());
});

/** Extensions accepted under the current policy, lowercase and without dots. */
export const allowedUploadExtensions = derived(uploadConfig, ($config) =>
  UPLOAD_CATEGORIES.filter((category) => $config.categories.includes(category.id)).flatMap(
    (category) => category.extensions
  )
);

/** `accept` attribute for file pickers, or '' when everything is allowed. */
export const uploadAccept = derived(allowedUploadExtensions, ($extensions) =>
  $extensions.map((ext) => `.${ext}`).join(',')
);

/** Render a byte count the way the upload settings talk about it. */
export function formatUploadSize(bytes: number): string {
  const mb = bytes / (1024 * 1024);
  if (mb >= 1) return `${Number.isInteger(mb) ? mb : mb.toFixed(1)} MB`;
  return `${Math.round(bytes / 1024)} KB`;
}

/**
 * Check a file against the current policy. Returns a user-facing reason when
 * the server would reject it, or `null` when it may be uploaded.
 */
export function describeUploadRejection(file: File): string | null {
  const config = get(uploadConfig);
  if (file.size > config.maxBytes) {
    return `This server accepts files up to ${formatUploadSize(config.maxBytes)}.`;
  }
  const extension = file.name.includes('.')
    ? file.name.split('.').pop()!.toLowerCase()
    : '';
  const category = UPLOAD_CATEGORIES.find((entry) => entry.extensions.includes(extension));
  if (!category) return 'This file type is not allowed on the server.';
  if (!config.categories.includes(category.id)) {
    return `${category.label} are not allowed on this server.`;
  }
  return null;
}

/**
 * Set the server-wide upload policy (requires Manage Server; enforced
 * server-side). Confirmation arrives as a broadcast upload-config frame which
 * updates the store.
 */
export function setUploadConfig(maxBytes: number, categories: string[]): void {
  chat.sendRaw({ type: 'set-upload-config', maxBytes, categories });
}
