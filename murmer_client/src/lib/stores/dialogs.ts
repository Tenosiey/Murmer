/*
  Promise-based in-app dialog service. WebView2 (Tauri on Windows) does not
  support window.prompt() at all, so every prompt/confirm/alert in the app
  goes through this store instead. A single <DialogHost /> in the root layout
  renders whatever dialog is active.

  Usage:
    const name = await dialogs.prompt({ title: 'New channel', label: 'Name' });
    const ok = await dialogs.confirm({ title: 'Ban user?', danger: true });
    const preset = await dialogs.select({ title: 'Voice quality', options });
    await dialogs.alert({ title: 'Screen share failed', message: '…' });
*/
import { writable } from 'svelte/store';

export interface DialogOption {
  value: string;
  label: string;
  description?: string;
}

export interface PromptDialogOptions {
  title: string;
  message?: string;
  label?: string;
  initial?: string;
  placeholder?: string;
  maxLength?: number;
  multiline?: boolean;
  confirmLabel?: string;
  /** Reject empty submissions (default true). */
  required?: boolean;
}

export interface ConfirmDialogOptions {
  title: string;
  message?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
}

export interface SelectDialogOptions {
  title: string;
  message?: string;
  options: DialogOption[];
  initial?: string;
  confirmLabel?: string;
}

export interface AlertDialogOptions {
  title: string;
  message?: string;
}

export type ActiveDialog =
  | { kind: 'prompt'; options: PromptDialogOptions; resolve: (value: string | null) => void }
  | { kind: 'confirm'; options: ConfirmDialogOptions; resolve: (value: boolean) => void }
  | { kind: 'select'; options: SelectDialogOptions; resolve: (value: string | null) => void }
  | { kind: 'alert'; options: AlertDialogOptions; resolve: () => void };

export const activeDialog = writable<ActiveDialog | null>(null);

/* Dialogs are modal one-at-a-time; a request made while another dialog is
   open queues behind it rather than replacing it. */
const queue: ActiveDialog[] = [];
let current: ActiveDialog | null = null;

function show(dialog: ActiveDialog) {
  if (current) {
    queue.push(dialog);
    return;
  }
  current = dialog;
  activeDialog.set(dialog);
}

function next() {
  current = queue.shift() ?? null;
  activeDialog.set(current);
}

/** Called by DialogHost when the user resolves the active dialog. */
export function settleDialog(dialog: ActiveDialog, value: unknown) {
  if (dialog !== current) return;
  switch (dialog.kind) {
    case 'prompt':
    case 'select':
      dialog.resolve((value as string | null) ?? null);
      break;
    case 'confirm':
      dialog.resolve(Boolean(value));
      break;
    case 'alert':
      dialog.resolve();
      break;
  }
  next();
}

export const dialogs = {
  prompt(options: PromptDialogOptions): Promise<string | null> {
    return new Promise((resolve) => show({ kind: 'prompt', options, resolve }));
  },
  confirm(options: ConfirmDialogOptions): Promise<boolean> {
    return new Promise((resolve) => show({ kind: 'confirm', options, resolve }));
  },
  select(options: SelectDialogOptions): Promise<string | null> {
    return new Promise((resolve) => show({ kind: 'select', options, resolve }));
  },
  alert(options: AlertDialogOptions): Promise<void> {
    return new Promise((resolve) => show({ kind: 'alert', options, resolve }));
  }
};
