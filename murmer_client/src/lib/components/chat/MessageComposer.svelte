<!--
  Message composer: auto-growing textarea, file picker with preview chip,
  reply banner, typing indicator and slash-command feedback. Owns only input
  presentation — send/reply/file state live in the chat page, which passes
  callbacks down.
-->
<script lang="ts">

  import type { Message } from '$lib/types';
  import { formatFileSize, searchResultPreview } from '$lib/chat/helpers';
  import { MESSAGE_INPUT_MAX_HEIGHT } from '$lib/chat/constants';


  interface Props {
    value?: string;
    replyingTo?: Message | null;
    typingLabel?: string | null;
    commandFeedback?: string | null;
    commandFeedbackType?: 'info' | 'error';
    pendingFile?: File | null;
    previewUrl?: string | null;
    onSend: () => void;
    onInput: () => void;
    onCancelReply: () => void;
    onFileSelected: (file: File | null) => void;
  }

  let {
    value = $bindable(''),
    replyingTo = null,
    typingLabel = null,
    commandFeedback = null,
    commandFeedbackType = 'info',
    pendingFile = null,
    previewUrl = null,
    onSend,
    onInput,
    onCancelReply,
    onFileSelected
  }: Props = $props();

  let fileInput: HTMLInputElement | undefined = $state();
  let textarea: HTMLTextAreaElement | undefined = $state();
  let scrollable = $state(false);

  export function focusInput() {
    textarea?.focus();
  }

  function autoResize() {
    if (!textarea) return;
    textarea.style.height = 'auto';
    const h = Math.min(textarea.scrollHeight, MESSAGE_INPUT_MAX_HEIGHT);
    textarea.style.height = h + 'px';
    scrollable = textarea.scrollHeight > h;
  }

  /* Shrink back after sends and programmatic clears, not just user input. */
  $effect(() => {
    void value;
    if (textarea) autoResize();
  });

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      onSend();
    } else if (event.key === 'Escape' && replyingTo) {
      onCancelReply();
    }
  }

  function handlePaste(event: ClipboardEvent) {
    const items = event.clipboardData?.items;
    if (!items) return;
    for (const item of items) {
      if (item.kind === 'file') {
        const file = item.getAsFile();
        if (file) {
          event.preventDefault();
          onFileSelected(file);
          return;
        }
      }
    }
  }

  function clearFile() {
    if (fileInput) fileInput.value = '';
    onFileSelected(null);
  }
</script>

<div class="composer">
  {#if commandFeedback}
    <div class={`command-feedback ${commandFeedbackType}`}>{commandFeedback}</div>
  {/if}

  {#if replyingTo}
    <div class="reply-bar">
      <span class="reply-bar-label">
        Replying to <strong>{replyingTo.user}</strong>
        <span class="reply-bar-preview">{searchResultPreview(replyingTo)}</span>
      </span>
      <button type="button" class="reply-bar-cancel" onclick={onCancelReply} aria-label="Cancel reply">
        ✕
      </button>
    </div>
  {/if}

  {#if typingLabel}
    <div class="typing-indicator" aria-live="polite">
      <span class="typing-dots" aria-hidden="true"><span></span><span></span><span></span></span>
      {typingLabel}
    </div>
  {/if}

  <textarea
    bind:this={textarea}
    bind:value
    class:scrollable
    rows="1"
    placeholder="Message"
    oninput={onInput}
    onpaste={handlePaste}
    onkeydown={handleKeydown}
  ></textarea>

  <!-- Hidden native input; the visible "Upload file" button proxies to it. -->
  <input
    type="file"
    class="sr-only"
    aria-hidden="true"
    tabindex="-1"
    bind:this={fileInput}
    onchange={() => onFileSelected(fileInput?.files?.[0] ?? null)}
  />

  <div class="controls">
    {#if pendingFile}
      <div class="preview-container">
        {#if previewUrl}
          <img src={previewUrl} alt="preview" class="preview" />
        {:else}
          <span class="file-chip" title={pendingFile.name}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48"/></svg>
            <span class="file-chip-name">{pendingFile.name}</span>
            {#if pendingFile.size > 0}
              <span class="file-chip-size">{formatFileSize(pendingFile.size)}</span>
            {/if}
          </span>
        {/if}
        <button class="preview-remove" onclick={clearFile} aria-label="Remove file">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>
    {/if}
    <div class="input-controls">
      <button
        type="button"
        class="file-button"
        title="Upload file"
        aria-label="Upload file"
        onclick={() => fileInput?.click()}
      >
        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" aria-hidden="true">
          <path stroke-linecap="round" stroke-linejoin="round" d="m2.25 15.75 5.159-5.159a2.25 2.25 0 0 1 3.182 0l5.159 5.159m-1.5-1.5 1.409-1.409a2.25 2.25 0 0 1 3.182 0l2.909 2.909m-18 3.75h16.5a1.5 1.5 0 0 0 1.5-1.5V6a1.5 1.5 0 0 0-1.5-1.5H3.75A1.5 1.5 0 0 0 2.25 6v12a1.5 1.5 0 0 0 1.5 1.5Zm10.5-11.25h.008v.008h-.008V8.25Zm.375 0a.375.375 0 1 1-.75 0 .375.375 0 0 1 .75 0Z" />
        </svg>
      </button>
      <button class="send" onclick={onSend} title="Send message" aria-label="Send message">
        <svg xmlns="http://www.w3.org/2000/svg" fill="currentColor" viewBox="0 0 24 24" aria-hidden="true">
          <path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z" />
        </svg>
      </button>
    </div>
  </div>
</div>

<style>
  .composer {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: var(--space-2);
    align-items: end;
    padding: var(--space-3) var(--space-4) var(--space-4);
    border-top: 1px solid var(--color-surface-outline);
    background: var(--color-surface);
  }

  .command-feedback {
    grid-column: 1 / -1;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    font-size: var(--text-sm);
    font-weight: 500;
    border: 1px solid color-mix(in srgb, var(--color-success) 30%, transparent);
    background: color-mix(in srgb, var(--color-success) 10%, transparent);
    color: var(--color-success);
  }

  .command-feedback.error {
    border-color: color-mix(in srgb, var(--color-error) 35%, transparent);
    background: color-mix(in srgb, var(--color-error) 10%, transparent);
    color: var(--color-error);
  }

  .reply-bar {
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--color-surface-raised);
    font-size: var(--text-sm);
    color: var(--color-muted);
  }

  .reply-bar-label {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    overflow: hidden;
  }

  .reply-bar-label strong {
    color: var(--color-on-surface);
  }

  .reply-bar-preview {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .reply-bar-cancel {
    flex-shrink: 0;
    border: none;
    background: transparent;
    color: var(--color-muted);
    cursor: pointer;
    font-size: var(--text-sm);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-xs);
  }

  .reply-bar-cancel:hover {
    color: var(--color-on-surface);
    background: var(--color-surface-elevated);
  }

  .typing-indicator {
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .typing-dots {
    display: inline-flex;
    gap: 0.1875rem;
  }

  .typing-dots span {
    width: 0.25rem;
    height: 0.25rem;
    border-radius: 50%;
    background: var(--color-muted);
    animation: typing-bounce 1.2s infinite ease-in-out;
  }

  .typing-dots span:nth-child(2) {
    animation-delay: 0.15s;
  }

  .typing-dots span:nth-child(3) {
    animation-delay: 0.3s;
  }

  @keyframes typing-bounce {
    0%,
    60%,
    100% {
      transform: translateY(0);
      opacity: 0.5;
    }
    30% {
      transform: translateY(-3px);
      opacity: 1;
    }
  }

  textarea {
    width: 100%;
    min-height: var(--control-height-lg);
    max-height: 360px;
    resize: none;
    overflow-y: hidden;
    overflow-x: hidden;
    border-radius: var(--radius-md);
    padding: var(--space-3) var(--space-4);
    line-height: 1.5;
  }

  textarea.scrollable {
    overflow-y: auto;
  }

  .controls {
    display: flex;
    align-items: flex-end;
    gap: var(--space-2);
  }

  .input-controls {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .preview-container {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-1);
    background: var(--color-surface-raised);
    padding: var(--space-2);
    border-radius: var(--radius-md);
    border: 1px dashed var(--color-outline-strong);
  }

  .preview-container img {
    max-width: 120px;
    max-height: 120px;
    border-radius: var(--radius-sm);
  }

  .preview-remove {
    background: transparent;
    color: var(--color-muted);
    border: none;
    display: inline-flex;
    padding: var(--space-1);
    border-radius: var(--radius-xs);
  }

  .preview-remove:hover {
    color: var(--color-error);
    background: color-mix(in srgb, var(--color-error) 12%, transparent);
  }

  .file-chip {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    max-width: 220px;
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--color-surface-elevated);
    font-size: var(--text-sm);
  }

  .file-chip svg {
    flex-shrink: 0;
    color: var(--color-primary);
  }

  .file-chip-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .file-chip-size {
    color: var(--color-muted);
    font-size: var(--text-xs);
    flex-shrink: 0;
  }

  .file-button,
  .send {
    width: var(--control-height-lg);
    height: var(--control-height-lg);
    flex-shrink: 0;
    border-radius: var(--radius-md);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }

  .file-button {
    border: 1px solid var(--color-surface-outline);
    background: var(--color-surface-raised);
    color: var(--color-on-surface-variant);
  }

  .file-button:hover {
    border-color: var(--color-outline-strong);
    color: var(--color-on-surface);
  }

  .send {
    border: none;
    background: var(--color-primary);
    color: var(--color-on-primary);
  }

  .send:hover {
    background: color-mix(in srgb, var(--color-primary) 88%, var(--color-on-surface));
  }

  .file-button svg,
  .send svg {
    width: 1.25rem;
    height: 1.25rem;
  }

  @media (max-width: 640px) {
    .composer {
      grid-template-columns: 1fr;
    }

    .controls {
      justify-content: flex-end;
    }
  }
</style>
