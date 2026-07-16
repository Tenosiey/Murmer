<!--
  Renders the active in-app dialog from the dialogs store: prompt, confirm,
  select or alert. Mounted once in the root layout. Escape or clicking the
  overlay cancels; Enter submits single-line prompts.
-->
<script lang="ts">
  import { tick } from 'svelte';
  import { fade, scale } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { activeDialog, settleDialog, type ActiveDialog } from '$lib/stores/dialogs';

  let value = $state('');
  let selected = $state('');
  let inputEl: HTMLInputElement | HTMLTextAreaElement | null = $state(null);
  let confirmEl: HTMLButtonElement | null = $state(null);
  let previousFocus: HTMLElement | null = $state(null);

  let dialog = $derived($activeDialog);

  /* Reset local state whenever a new dialog becomes active and move focus
     into it; focus returns to the previously focused element on close. */
  let seen: ActiveDialog | null = $state(null);
  $effect(() => {
    if (dialog !== seen) {
      seen = dialog;
      if (dialog) {
        previousFocus = document.activeElement as HTMLElement | null;
        value = dialog.kind === 'prompt' ? (dialog.options.initial ?? '') : '';
        selected =
          dialog.kind === 'select'
            ? (dialog.options.initial ?? dialog.options.options[0]?.value ?? '')
            : '';
        tick().then(() => {
          (inputEl ?? confirmEl)?.focus();
          if (inputEl && 'select' in inputEl) inputEl.select();
        });
      } else {
        previousFocus?.focus?.();
        previousFocus = null;
      }
    }
  });

  function cancel() {
    if (!dialog) return;
    settleDialog(dialog, dialog.kind === 'confirm' ? false : null);
  }

  function submit() {
    if (!dialog) return;
    switch (dialog.kind) {
      case 'prompt': {
        const required = dialog.options.required ?? true;
        if (required && value.trim() === '') return;
        settleDialog(dialog, value);
        break;
      }
      case 'select':
        settleDialog(dialog, selected);
        break;
      case 'confirm':
        settleDialog(dialog, true);
        break;
      case 'alert':
        settleDialog(dialog, undefined);
        break;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.stopPropagation();
      cancel();
    } else if (event.key === 'Enter' && !event.shiftKey) {
      // Multiline prompts keep Enter for newlines; submit is Ctrl+Enter.
      if (dialog?.kind === 'prompt' && dialog.options.multiline && !event.ctrlKey) return;
      event.preventDefault();
      submit();
    }
  }

  let promptValid =
    $derived(dialog?.kind !== 'prompt' || !(dialog.options.required ?? true) || value.trim() !== '');
</script>

{#if dialog}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="overlay"
    role="presentation"
    transition:fade={{ duration: 120 }}
    onmousedown={(event) => {
      if (event.target === event.currentTarget) cancel();
    }}
    onkeydown={handleKeydown}
  >
    <div
      class="dialog"
      role={dialog.kind === 'alert' ? 'alertdialog' : 'dialog'}
      aria-modal="true"
      aria-labelledby="dialog-title"
      transition:scale={{ start: 0.96, duration: 140, easing: cubicOut }}
    >
      <h2 id="dialog-title">{dialog.options.title}</h2>

      {#if dialog.options.message}
        <p class="message">{dialog.options.message}</p>
      {/if}

      {#if dialog.kind === 'prompt'}
        <label class="field">
          {#if dialog.options.label}<span>{dialog.options.label}</span>{/if}
          {#if dialog.options.multiline}
            <textarea
              bind:this={inputEl}
              bind:value
              rows="4"
              placeholder={dialog.options.placeholder ?? ''}
              maxlength={dialog.options.maxLength}
            ></textarea>
          {:else}
            <input
              bind:this={inputEl}
              bind:value
              type="text"
              placeholder={dialog.options.placeholder ?? ''}
              maxlength={dialog.options.maxLength}
            />
          {/if}
        </label>
        {#if dialog.options.maxLength && value.length > dialog.options.maxLength * 0.8}
          <span class="char-count">{value.length}/{dialog.options.maxLength}</span>
        {/if}
      {:else if dialog.kind === 'select'}
        <div class="options" role="radiogroup" aria-labelledby="dialog-title">
          {#each dialog.options.options as option (option.value)}
            <label class="option" class:selected={selected === option.value}>
              <input type="radio" name="dialog-select" value={option.value} bind:group={selected} />
              <span class="option-copy">
                <span class="option-label">{option.label}</span>
                {#if option.description}
                  <span class="option-description">{option.description}</span>
                {/if}
              </span>
            </label>
          {/each}
        </div>
      {/if}

      <div class="actions">
        {#if dialog.kind !== 'alert'}
          <button type="button" class="btn" onclick={cancel}>
            {(dialog.kind === 'confirm' && dialog.options.cancelLabel) || 'Cancel'}
          </button>
        {/if}
        <button
          type="button"
          bind:this={confirmEl}
          class="btn {dialog.kind === 'confirm' && dialog.options.danger ? 'btn-danger' : 'btn-primary'}"
          disabled={!promptValid}
          onclick={submit}
        >
          {dialog.kind === 'alert'
            ? 'OK'
            : (dialog.kind === 'prompt' || dialog.kind === 'select' || dialog.kind === 'confirm'
                ? (dialog.options.confirmLabel ?? 'OK')
                : 'OK')}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: var(--z-modal);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-5);
    background: var(--color-overlay);
    backdrop-filter: var(--blur-elevated);
  }

  .dialog {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    width: min(420px, 100%);
    padding: var(--space-5);
    border-radius: var(--radius-lg);
    background: var(--color-surface-elevated);
    border: 1px solid var(--color-surface-outline);
    box-shadow: var(--shadow-lg);
  }

  h2 {
    font-size: var(--text-lg);
  }

  .message {
    margin: 0;
    color: var(--color-muted);
    line-height: 1.5;
  }

  .char-count {
    align-self: flex-end;
    font-size: var(--text-xs);
    color: var(--color-muted);
    font-family: var(--font-mono);
  }

  .options {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    max-height: 50vh;
    overflow-y: auto;
  }

  .option {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    border: 1px solid transparent;
    cursor: pointer;
  }

  .option:hover {
    background: var(--color-surface-raised);
  }

  .option.selected {
    background: var(--color-primary-container);
    border-color: color-mix(in srgb, var(--color-primary) 45%, transparent);
  }

  .option input {
    /* The styled row is the visual control; keep the radio for semantics. */
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }

  .option:has(:global(input:focus-visible)) {
    outline: 2px solid var(--color-primary);
    outline-offset: 2px;
  }

  .option-copy {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .option-label {
    font-weight: 500;
  }

  .option-description {
    font-size: var(--text-sm);
    color: var(--color-muted);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
