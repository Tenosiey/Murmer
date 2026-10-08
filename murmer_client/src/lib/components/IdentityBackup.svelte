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
  import { t } from '$lib/i18n';

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
    return t('backup.genericError');
  }

  async function saveFile() {
    if (passphrase.length < MIN_PASSPHRASE) {
      error = t('backup.tooShort', { min: MIN_PASSPHRASE });
      return;
    }
    if (passphrase !== passphraseAgain) {
      error = t('backup.mismatch');
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
      notice = t('backup.saved');
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
        if (!restoreFileText) throw new IdentityError(t('backup.chooseFirst'));
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
        notice = t('backup.sameIdentity');
        return;
      }

      const confirmed = await dialogs.confirm({
        title: t('backup.replaceTitle'),
        message: t('backup.replaceMessage'),
        confirmLabel: t('backup.replaceConfirm'),
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
  <span class="setting-label">{t('backup.title')}</span>
  <div class="setting-description">
    {t('backup.intro')}
  </div>

  <div class="backup-actions">
    <button class="btn" onclick={() => openPanel('file')} disabled={busy}>
      {t('backup.saveRecoveryFile')}
    </button>
    <button class="btn" onclick={() => openPanel('phrase')} disabled={busy}>
      {t('backup.showRecoveryPhrase')}
    </button>
    <button class="btn" onclick={() => openPanel('restore')} disabled={busy}>
      {t('backup.restore')}
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
        {t('backup.fileHint')}
      </div>
      <label class="backup-field">
        <span>{t('backup.passphrase')}</span>
        <input class="field" type="password" bind:value={passphrase} autocomplete="new-password" />
      </label>
      <label class="backup-field">
        <span>{t('backup.passphraseAgain')}</span>
        <input
          class="field"
          type="password"
          bind:value={passphraseAgain}
          autocomplete="new-password"
        />
      </label>
      <div class="backup-actions">
        <button class="btn btn-primary" onclick={saveFile} disabled={busy}>
          {busy ? t('backup.encrypting') : t('backup.saveFile')}
        </button>
        <button class="btn btn-ghost" onclick={() => openPanel('none')} disabled={busy}>
          {t('backup.cancel')}
        </button>
      </div>
    </div>
  {/if}

  {#if panel === 'phrase'}
    <div class="backup-panel">
      {#if phrase}
        <div class="setting-description">
          {t('backup.writeDown')}
        </div>
        <ol class="phrase-grid">
          {#each phrase.split(' ') as word, index (index)}
            <li><span class="phrase-index">{index + 1}</span>{word}</li>
          {/each}
        </ol>
        <div class="backup-actions">
          <button class="btn" onclick={copyPhrase}>
            {phraseCopied ? t('backup.copied') : t('backup.copyPhrase')}
          </button>
          <button class="btn btn-ghost" onclick={() => openPanel('none')}>{t('backup.done')}</button>
        </div>
      {:else}
        <div class="setting-description">
          {t('backup.revealWarning')}
        </div>
        <div class="backup-actions">
          <button class="btn btn-primary" onclick={revealPhrase}>{t('backup.showPhrase')}</button>
          <button class="btn btn-ghost" onclick={() => openPanel('none')}>{t('backup.cancel')}</button>
        </div>
      {/if}
    </div>
  {/if}

  {#if panel === 'restore'}
    <div class="backup-panel">
      <div class="restore-tabs" role="tablist" aria-label={t('backup.restoreFrom')}>
        <button
          class="btn btn-ghost"
          class:selected={restoreSource === 'file'}
          role="tab"
          aria-selected={restoreSource === 'file'}
          onclick={() => ((restoreSource = 'file'), (error = ''))}
        >
          {t('backup.recoveryFile')}
        </button>
        <button
          class="btn btn-ghost"
          class:selected={restoreSource === 'phrase'}
          role="tab"
          aria-selected={restoreSource === 'phrase'}
          onclick={() => ((restoreSource = 'phrase'), (error = ''))}
        >
          {t('backup.recoveryPhrase')}
        </button>
      </div>

      {#if restoreSource === 'file'}
        <div class="backup-actions">
          <button class="btn" onclick={() => fileInput?.click()} disabled={busy}>
            {restoreFileName || t('backup.chooseFile')}
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
          <span>{t('backup.passphrase')}</span>
          <input
            class="field"
            type="password"
            bind:value={restorePassphrase}
            autocomplete="current-password"
          />
        </label>
      {:else}
        <label class="backup-field">
          <span>{t('backup.recoveryPhrase')}</span>
          <textarea
            class="field phrase-input"
            rows="3"
            placeholder={t('backup.phrasePlaceholder')}
            bind:value={restorePhrase}
          ></textarea>
        </label>
      {/if}

      <div class="backup-actions">
        <button class="btn btn-danger" onclick={restore} disabled={busy}>
          {busy ? t('backup.restoring') : t('backup.restoreIdentity')}
        </button>
        <button class="btn btn-ghost" onclick={() => openPanel('none')} disabled={busy}>
          {t('backup.cancel')}
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
