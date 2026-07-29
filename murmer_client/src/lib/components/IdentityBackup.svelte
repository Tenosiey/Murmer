<!--
  Backup and restore of the account identity.

  The key in localStorage is the account on every server at once, and the only
  thing that can read the DMs sent to it. Until this existed there was no way
  to copy it to a second machine and no way to get it back after a reinstall —
  a lost key meant a lost account, recoverable only by asking the operator to
  release the name, and DM history not at all.

  Restoring is destructive in a way little else in the app is, so it is the one
  action here that asks for confirmation and then reloads: the connection, the
  voice manager and every store were built around the old key, and a reload is
  the only way to be sure none of them keep using it.
-->
<script lang="ts">
  import {
    IdentityError,
    exportIdentityFile,
    importIdentityFile,
    phraseToSeed,
    seedToPhrase,
    suggestedFileName
  } from '$lib/identity';
  import { keyPairFromSeed, loadKeyPair, replaceKeyPair, seedOf } from '$lib/keypair';
  import { session } from '$lib/stores/session';
  import { dialogs } from '$lib/stores/dialogs';

  /** Shortest passphrase worth calling one. The file is the account. */
  const MIN_PASSPHRASE = 8;

  type Panel = 'none' | 'file' | 'phrase' | 'restore';

  let panel = $state<Panel>('none');
  let busy = $state(false);
  let error = $state('');
  let notice = $state('');

  let passphrase = $state('');
  let passphraseAgain = $state('');

  let phrase = $state('');
  let phraseCopied = $state(false);

  let restoreSource = $state<'file' | 'phrase'>('file');
  let restoreFileName = $state('');
  let restoreFileText = $state('');
  let restorePassphrase = $state('');
  let restorePhrase = $state('');
  let fileInput = $state<HTMLInputElement | null>(null);

  function openPanel(next: Panel) {
    // Never carry a passphrase or a revealed phrase across panels.
    passphrase = '';
    passphraseAgain = '';
    phrase = '';
    phraseCopied = false;
    restoreFileName = '';
    restoreFileText = '';
    restorePassphrase = '';
    restorePhrase = '';
    error = '';
    notice = '';
    panel = panel === next ? 'none' : next;
  }

  /** The message to show for a failure, without leaking anything internal. */
  function describe(err: unknown): string {
    if (err instanceof IdentityError) return err.message;
    console.error('Identity backup failed', err);
    return 'Something went wrong. Please try again.';
  }

  async function saveFile() {
    if (passphrase.length < MIN_PASSPHRASE) {
      error = `Use at least ${MIN_PASSPHRASE} characters — this file is your account.`;
      return;
    }
    if (passphrase !== passphraseAgain) {
      error = 'The two passphrases do not match.';
      return;
    }

    busy = true;
    error = '';
    try {
      const user = $session.user ?? '';
      const text = await exportIdentityFile({ seed: seedOf(loadKeyPair()), user }, passphrase);
      const blob = new Blob([text], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = url;
      link.download = suggestedFileName(user);
      link.click();
      URL.revokeObjectURL(url);

      passphrase = '';
      passphraseAgain = '';
      panel = 'none';
      notice = 'Recovery file saved. Keep it somewhere you would keep a password.';
    } catch (err) {
      error = describe(err);
    } finally {
      busy = false;
    }
  }

  function revealPhrase() {
    error = '';
    try {
      phrase = seedToPhrase(seedOf(loadKeyPair()));
    } catch (err) {
      error = describe(err);
    }
  }

  async function copyPhrase() {
    try {
      await navigator.clipboard.writeText(phrase);
      phraseCopied = true;
      setTimeout(() => (phraseCopied = false), 2000);
    } catch (err) {
      error = describe(err);
    }
  }

  async function pickFile(event: Event) {
    const file = (event.currentTarget as HTMLInputElement).files?.[0];
    if (!file) return;
    error = '';
    restoreFileName = file.name;
    try {
      restoreFileText = await file.text();
    } catch (err) {
      restoreFileText = '';
      error = describe(err);
    }
  }

  async function restore() {
    busy = true;
    error = '';
    try {
      let seed: Uint8Array;
      let user = '';

      if (restoreSource === 'file') {
        if (!restoreFileText) throw new IdentityError('Choose a recovery file first.');
        const identity = await importIdentityFile(restoreFileText, restorePassphrase);
        seed = identity.seed;
        user = identity.user;
      } else {
        seed = phraseToSeed(restorePhrase);
      }

      // Restoring the identity already in use is a no-op worth naming: the
      // alternative is a reload that looks like it did something.
      if (keyPairFromSeed(seed).publicKey === loadKeyPair().publicKey) {
        busy = false;
        error = '';
        notice = 'That backup holds the identity this device is already using.';
        return;
      }

      const confirmed = await dialogs.confirm({
        title: 'Replace this device’s identity?',
        message:
          'The key on this device will be replaced. Any account it currently owns becomes ' +
          'unreachable from here without its own backup, and direct messages addressed to ' +
          'it can no longer be read. Murmer will reload.',
        confirmLabel: 'Replace identity',
        danger: true
      });
      if (!confirmed) {
        busy = false;
        return;
      }

      replaceKeyPair(seed);
      if (user) session.set({ user });
      // Everything already running authenticated with the old key; a reload is
      // the only honest way to rebuild it all against the new one.
      location.reload();
    } catch (err) {
      error = describe(err);
      busy = false;
    }
  }
</script>

<div class="setting-group">
  <span class="setting-label">Backup &amp; recovery</span>
  <div class="setting-description">
    Your key is your account on every server you use, and the only thing that can read the
    direct messages sent to you. It lives on this device only — back it up, or a reinstall or a
    new machine means starting over under a new name.
  </div>

  <div class="backup-actions">
    <button class="btn" onclick={() => openPanel('file')} disabled={busy}>
      Save recovery file
    </button>
    <button class="btn" onclick={() => openPanel('phrase')} disabled={busy}>
      Show recovery phrase
    </button>
    <button class="btn" onclick={() => openPanel('restore')} disabled={busy}>
      Restore from backup
    </button>
  </div>

  {#if notice}
    <div class="setting-description backup-notice" role="status">{notice}</div>
  {/if}
  {#if error}
    <div class="setting-description backup-error" role="alert">{error}</div>
  {/if}

  {#if panel === 'file'}
    <div class="backup-panel">
      <div class="setting-description">
        The file is encrypted with this passphrase and holds your key and account name. If you
        forget the passphrase the file is worthless — there is no way to recover it.
      </div>
      <label class="backup-field">
        <span>Passphrase</span>
        <input class="field" type="password" bind:value={passphrase} autocomplete="new-password" />
      </label>
      <label class="backup-field">
        <span>Passphrase again</span>
        <input
          class="field"
          type="password"
          bind:value={passphraseAgain}
          autocomplete="new-password"
        />
      </label>
      <div class="backup-actions">
        <button class="btn btn-primary" onclick={saveFile} disabled={busy}>
          {busy ? 'Encrypting…' : 'Save file'}
        </button>
        <button class="btn btn-ghost" onclick={() => openPanel('none')} disabled={busy}>
          Cancel
        </button>
      </div>
    </div>
  {/if}

  {#if panel === 'phrase'}
    <div class="backup-panel">
      {#if phrase}
        <div class="setting-description">
          Write these 24 words down in order and keep them somewhere private. Anyone who has
          them owns your account.
        </div>
        <ol class="phrase-grid">
          {#each phrase.split(' ') as word, index (index)}
            <li><span class="phrase-index">{index + 1}</span>{word}</li>
          {/each}
        </ol>
        <div class="backup-actions">
          <button class="btn" onclick={copyPhrase}>
            {phraseCopied ? 'Copied' : 'Copy phrase'}
          </button>
          <button class="btn btn-ghost" onclick={() => openPanel('none')}>Done</button>
        </div>
      {:else}
        <div class="setting-description">
          The phrase is shown on screen. Make sure nobody is watching and that you are not
          sharing your screen.
        </div>
        <div class="backup-actions">
          <button class="btn btn-primary" onclick={revealPhrase}>Show the phrase</button>
          <button class="btn btn-ghost" onclick={() => openPanel('none')}>Cancel</button>
        </div>
      {/if}
    </div>
  {/if}

  {#if panel === 'restore'}
    <div class="backup-panel">
      <div class="restore-tabs" role="tablist" aria-label="Restore from">
        <button
          class="btn btn-ghost"
          class:selected={restoreSource === 'file'}
          role="tab"
          aria-selected={restoreSource === 'file'}
          onclick={() => ((restoreSource = 'file'), (error = ''))}
        >
          Recovery file
        </button>
        <button
          class="btn btn-ghost"
          class:selected={restoreSource === 'phrase'}
          role="tab"
          aria-selected={restoreSource === 'phrase'}
          onclick={() => ((restoreSource = 'phrase'), (error = ''))}
        >
          Recovery phrase
        </button>
      </div>

      {#if restoreSource === 'file'}
        <div class="backup-actions">
          <button class="btn" onclick={() => fileInput?.click()} disabled={busy}>
            {restoreFileName || 'Choose file…'}
          </button>
          <input
            bind:this={fileInput}
            type="file"
            accept=".murmer-identity,application/json"
            hidden
            onchange={pickFile}
          />
        </div>
        <label class="backup-field">
          <span>Passphrase</span>
          <input
            class="field"
            type="password"
            bind:value={restorePassphrase}
            autocomplete="current-password"
          />
        </label>
      {:else}
        <label class="backup-field">
          <span>Recovery phrase</span>
          <textarea
            class="field phrase-input"
            rows="3"
            placeholder="The 24 words, in order, separated by spaces"
            bind:value={restorePhrase}
          ></textarea>
        </label>
      {/if}

      <div class="backup-actions">
        <button class="btn btn-danger" onclick={restore} disabled={busy}>
          {busy ? 'Restoring…' : 'Restore identity'}
        </button>
        <button class="btn btn-ghost" onclick={() => openPanel('none')} disabled={busy}>
          Cancel
        </button>
      </div>
    </div>
  {/if}
</div>

<style>
  .backup-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }

  .backup-panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-top: var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
  }

  .backup-field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font-size: var(--text-sm);
    color: var(--color-on-surface-variant);
  }

  .backup-notice {
    color: var(--color-success);
  }

  .backup-error {
    color: var(--color-error);
  }

  .restore-tabs {
    display: flex;
    gap: var(--space-1);
  }

  .restore-tabs .selected {
    background: var(--color-surface-variant);
    color: var(--color-on-surface);
  }

  .phrase-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(9rem, 1fr));
    gap: var(--space-1) var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .phrase-grid li {
    display: flex;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--color-surface-variant);
  }

  .phrase-index {
    min-width: 1.5rem;
    color: var(--color-muted);
    text-align: right;
  }

  .phrase-input {
    font-family: var(--font-mono);
    resize: vertical;
  }
</style>
