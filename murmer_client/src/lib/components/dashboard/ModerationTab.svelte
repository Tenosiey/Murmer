<script lang="ts">
  import { onMount, onDestroy, untrack } from 'svelte';
  import { chat } from '$lib/stores/chat';
  import { dialogs } from '$lib/stores/dialogs';
  import { describeServerError } from '$lib/errors';
  import { displayNames } from '$lib/stores/profiles';
  import {
    MAX_MESSAGE_LENGTH,
    MIN_CONFIGURABLE_MESSAGE_LENGTH,
    MAX_SLOW_MODE_SECONDS,
    MAX_PROFANITY_WORDS,
    MAX_PROFANITY_WORD_LEN,
    AUTOMOD_ACTIONS,
    AUTOMOD_KINDS,
    DEFAULT_AUTOMOD_MUTE_SECONDS,
    MAX_AUTOMOD_NAME_LEN,
    MAX_AUTOMOD_PATTERN_LEN,
    MAX_AUTOMOD_RULES,
    MAX_MUTE_SECONDS,
    MIN_MUTE_SECONDS
  } from '$lib/chat/constants';
  import {
    chatSettings,
    requestChatSettings,
    setChatSettings
  } from '$lib/stores/chatSettings';
  import {
    automodRules,
    clampMuteSeconds,
    requestAutomodRules,
    setAutomodRules,
    type AutomodRule
  } from '$lib/stores/automod';
  import { bans } from '$lib/stores/bans';
  import {
    PERMISSIONS,
    hasPermission
  } from '$lib/chat/permissions';
  import type { Message } from '$lib/types';

  interface Props {
    active: boolean;
    /** The viewer's effective permission mask. */
    permissions: number;
  }

  let { active, permissions }: Props = $props();

  // Slow mode, the message length cap and the profanity filter. Editing needs
  // MANAGE_SERVER, which the Moderation tab itself does not (it opens for
  // BAN_MEMBERS so moderators can work the ban list), so the settings block
  // is gated separately.
  let canManageServer = $derived(hasPermission(permissions, PERMISSIONS.MANAGE_SERVER));

  let slowModeSeconds = $state(0);
  let maxMessageLength = $state(MAX_MESSAGE_LENGTH);
  let profanityFilter = $state(false);
  /** The word list as edited: one word per line. */
  let profanityWords = $state('');
  let chatFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);
  let chatSavePending = $state(false);

  /** Split the editor's text into normalized words the way the server does. */
  function parseWordList(raw: string): string[] {
    const words: string[] = [];
    for (const entry of raw.split(/[\n,]/)) {
      const word = entry.trim().toLowerCase();
      if (!word || word.length > MAX_PROFANITY_WORD_LEN || /\s/.test(word)) continue;
      if (!words.includes(word)) words.push(word);
      if (words.length === MAX_PROFANITY_WORDS) break;
    }
    return words;
  }

  let chatWordsDraft = $derived(parseWordList(profanityWords));

  // (Re-)fill the form from the server's answer. `words === null` means the
  // list has not been disclosed yet, so the editor stays empty and disabled
  // rather than offering to save an empty list over a real one.
  function loadChatSettings() {
    const current = $chatSettings;
    slowModeSeconds = current.slowModeSeconds;
    maxMessageLength = current.maxMessageLength;
    profanityFilter = current.profanityFilter;
    profanityWords = (current.words ?? []).join('\n');
    chatFeedback = null;
    chatSavePending = false;
  }

  /** Whether the manager-only word list has been taken into the editor. */
  let wordsLoaded = $state(false);

  // Ask for the full policy (word list included) whenever the tab is opened,
  // and fill the form with what is already known.
  $effect(() => {
    if (active) {
      untrack(() => {
        if (canManageServer) {
          requestChatSettings();
          requestAutomodRules();
        }
        bans.refresh();
        wordsLoaded = false;
        loadChatSettings();
        automodLoaded = false;
        automodDraft = [];
        automodFeedback = null;
        automodSavePending = false;
      });
    }
  });

  // The word list only travels in the answer to that request, which lands
  // after the form was filled — take it once, then leave the editor alone so
  // a later broadcast cannot clobber what the user is typing.
  $effect(() => {
    const words = $chatSettings.words;
    if (!active || words === null) return;
    untrack(() => {
      if (wordsLoaded) return;
      wordsLoaded = true;
      profanityWords = words.join('\n');
    });
  });

  // A broadcast matching the submitted values confirms the save.
  $effect(() => {
    if (chatSavePending && !chatDirty) {
      chatSavePending = false;
      chatFeedback = { text: 'Changes saved.', kind: 'info' };
    }
  });

  let chatDirty = $derived.by(() => {
    const current = $chatSettings;
    const words = current.words ?? [];
    return (
      slowModeSeconds !== current.slowModeSeconds ||
      maxMessageLength !== current.maxMessageLength ||
      profanityFilter !== current.profanityFilter ||
      chatWordsDraft.length !== words.length ||
      chatWordsDraft.some((word, index) => word !== words[index])
    );
  });

  function saveChatSettings() {
    const seconds = Number.isFinite(slowModeSeconds) ? Math.round(slowModeSeconds) : NaN;
    if (Number.isNaN(seconds) || seconds < 0 || seconds > MAX_SLOW_MODE_SECONDS) {
      chatFeedback = {
        text: `Slow mode must be between 0 and ${MAX_SLOW_MODE_SECONDS} seconds.`,
        kind: 'error'
      };
      return;
    }
    const length = Number.isFinite(maxMessageLength) ? Math.round(maxMessageLength) : NaN;
    if (
      Number.isNaN(length) ||
      length < MIN_CONFIGURABLE_MESSAGE_LENGTH ||
      length > MAX_MESSAGE_LENGTH
    ) {
      chatFeedback = {
        text: `The message limit must be between ${MIN_CONFIGURABLE_MESSAGE_LENGTH} and ${MAX_MESSAGE_LENGTH} characters.`,
        kind: 'error'
      };
      return;
    }
    chatFeedback = null;
    chatSavePending = true;
    setChatSettings({
      slowModeSeconds: seconds,
      maxMessageLength: length,
      profanityFilter,
      words: chatWordsDraft
    });
  }

  /** Render a slow mode interval the way people talk about it. */
  function describeInterval(seconds: number): string {
    if (seconds <= 0) return 'off';
    if (seconds < 60) return `${seconds}s`;
    if (seconds < 3600) {
      const minutes = Math.round(seconds / 60);
      return `${minutes} minute${minutes === 1 ? '' : 's'}`;
    }
    const hours = seconds / 3600;
    return `${Number.isInteger(hours) ? hours : hours.toFixed(1)} hour${hours === 1 ? '' : 's'}`;
  }

  function formatBanDate(value: string | null): string {
    if (!value) return 'unknown date';
    const parsed = new Date(value);
    return Number.isNaN(parsed.getTime()) ? 'unknown date' : parsed.toLocaleDateString();
  }

  async function liftBan(user: string) {
    const ok = await dialogs.confirm({
      title: `Unban ${user}?`,
      message: 'They will be able to join this server again.',
      confirmLabel: 'Unban'
    });
    if (!ok) return;
    bans.unban(user);
  }

  // The general form of the word list above: a pattern plus what to do about
  // it. Same gate as the rest of the chat policy (MANAGE_SERVER), and the
  // same shape — the draft below is local, the server's answer is the truth,
  // and a rule the server refuses must never end up looking saved.
  let automodDraft: AutomodRule[] = $state([]);
  let automodFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);
  let automodSavePending = $state(false);
  /** Whether the manager-only rule list has been taken into the editor. */
  let automodLoaded = $state(false);

  // The rules only travel in the answer to `get-automod-rules`, which lands
  // after the tab opened — take them once, then leave the editor alone so a
  // later answer cannot clobber a half-written rule.
  $effect(() => {
    const rules = $automodRules;
    if (!active || rules === null) return;
    untrack(() => {
      if (automodLoaded) return;
      automodLoaded = true;
      automodDraft = rules.map((rule) => ({ ...rule }));
    });
  });

  let automodDirty = $derived.by(() => {
    const current = $automodRules;
    if (current === null) return false;
    if (current.length !== automodDraft.length) return true;
    return automodDraft.some((rule, index) => {
      const stored = current[index];
      return (
        rule.name.trim() !== stored.name ||
        rule.pattern.trim() !== stored.pattern ||
        rule.kind !== stored.kind ||
        rule.action !== stored.action ||
        rule.enabled !== stored.enabled ||
        clampMuteSeconds(rule.muteSeconds) !== stored.muteSeconds
      );
    });
  });

  // The server's answer matching the draft confirms the save.
  $effect(() => {
    if (automodSavePending && !automodDirty) {
      automodSavePending = false;
      automodFeedback = { text: 'Rules saved.', kind: 'info' };
    }
  });

  function addAutomodRule() {
    if (automodDraft.length >= MAX_AUTOMOD_RULES) return;
    automodDraft = [
      ...automodDraft,
      {
        name: '',
        pattern: '',
        kind: 'word',
        action: 'delete',
        muteSeconds: DEFAULT_AUTOMOD_MUTE_SECONDS,
        enabled: true
      }
    ];
    automodFeedback = null;
  }

  function removeAutomodRule(index: number) {
    automodDraft = automodDraft.filter((_, position) => position !== index);
    automodFeedback = null;
  }

  /**
   * Why the draft cannot be saved, or null. Cosmetic — the server checks all
   * of it again, including whether a regular expression compiles, which only
   * it can answer.
   */
  function describeAutomodProblem(rules: AutomodRule[]): string | null {
    for (const [index, rule] of rules.entries()) {
      const position = index + 1;
      const pattern = rule.pattern.trim();
      if (!pattern) return `Rule ${position} has no pattern.`;
      if (pattern.length > MAX_AUTOMOD_PATTERN_LEN) {
        return `Rule ${position} has a pattern longer than ${MAX_AUTOMOD_PATTERN_LEN} characters.`;
      }
      if (rule.name.trim().length > MAX_AUTOMOD_NAME_LEN) {
        return `Rule ${position} has a name longer than ${MAX_AUTOMOD_NAME_LEN} characters.`;
      }
      // A whole-word pattern is compared against single words, so one with a
      // space in it would be saved and then never match anything.
      if (rule.kind === 'word' && /\s/.test(pattern)) {
        return `Rule ${position} matches a whole word, so its pattern cannot contain spaces.`;
      }
    }
    return null;
  }

  function saveAutomodRules() {
    const problem = describeAutomodProblem(automodDraft);
    if (problem) {
      automodFeedback = { text: problem, kind: 'error' };
      return;
    }
    automodFeedback = null;
    automodSavePending = true;
    // Role-checked server-side; the confirmation is the automod-rules answer.
    setAutomodRules(
      automodDraft.map((rule) => ({
        ...rule,
        name: rule.name.trim(),
        pattern: rule.pattern.trim(),
        muteSeconds: clampMuteSeconds(rule.muteSeconds)
      }))
    );
  }

  /** Render a mute duration the way people talk about it. */
  function describeMute(seconds: number): string {
    const clamped = clampMuteSeconds(seconds);
    if (clamped < 60) return `${clamped}s`;
    if (clamped < 3600) return `${Math.round(clamped / 60)} minutes`;
    if (clamped < 86_400) {
      const hours = clamped / 3600;
      return `${Number.isInteger(hours) ? hours : hours.toFixed(1)} hours`;
    }
    const days = clamped / 86_400;
    return `${Number.isInteger(days) ? days : days.toFixed(1)} days`;
  }

  // `automod-blocked` is deliberately absent: that one answers a chat message,
  // not anything this dashboard sent.
  const AUTOMOD_ERROR_CODES = new Set([
    'automod-permission-denied',
    'invalid-automod-rules',
    'invalid-automod-pattern',
    'automod-update-failed'
  ]);

  const CHAT_SETTINGS_ERROR_CODES = new Set([
    'chat-settings-permission-denied',
    'invalid-chat-settings',
    'chat-settings-update-failed'
  ]);

  const MODERATION_ERROR_CODES = new Set([
    'moderation-permission-denied',
    'moderation-target-not-found',
    'moderation-target-protected',
    'moderation-failed'
  ]);

  function handleServerError(msg: Message) {
    const code = msg.message;
    if (typeof code !== 'string') return;
    if (AUTOMOD_ERROR_CODES.has(code)) {
      automodSavePending = false;
      automodFeedback = { text: describeServerError(code), kind: 'error' };
    } else if (CHAT_SETTINGS_ERROR_CODES.has(code) || MODERATION_ERROR_CODES.has(code)) {
      chatSavePending = false;
      chatFeedback = { text: describeServerError(code), kind: 'error' };
    }
  }

  onMount(() => chat.on('error', handleServerError));
  onDestroy(() => chat.off('error', handleServerError));
</script>

{#if active}
  <div class="settings-section">
    <h3 class="section-title">Moderation</h3>
    {#if canManageServer}
      <div class="setting-group">
        <label class="setting-label" for="slow-mode">Slow mode</label>
        <input
          id="slow-mode"
          type="number"
          bind:value={slowModeSeconds}
          min="0"
          max={MAX_SLOW_MODE_SECONDS}
          step="1"
        />
        <div class="setting-description">
          Seconds each member must wait between messages ({describeInterval(
            slowModeSeconds
          )}). 0 turns slow mode off. Members who can manage messages are exempt.
        </div>
      </div>
      <div class="setting-group">
        <label class="setting-label" for="max-message-length">Max message length</label>
        <input
          id="max-message-length"
          type="number"
          bind:value={maxMessageLength}
          min={MIN_CONFIGURABLE_MESSAGE_LENGTH}
          max={MAX_MESSAGE_LENGTH}
          step="1"
        />
        <div class="setting-description">
          Characters per message, between {MIN_CONFIGURABLE_MESSAGE_LENGTH} and
          {MAX_MESSAGE_LENGTH}. The server rejects anything longer, so this can only
          tighten the built-in limit.
        </div>
      </div>
      <div class="setting-group">
        <label class="toggle-row">
          <input type="checkbox" bind:checked={profanityFilter} />
          <span class="toggle-text">
            <span class="toggle-label">Profanity filter</span>
            <span class="toggle-description">
              Replace filtered words with asterisks in new messages. Masking happens on
              the server before a message is stored, so the original never reaches
              anyone — including whoever edits it afterwards.
            </span>
          </span>
        </label>
      </div>
      <div class="setting-group">
        <label class="setting-label" for="profanity-words">Filtered words</label>
        <textarea
          id="profanity-words"
          rows="4"
          spellcheck="false"
          autocomplete="off"
          bind:value={profanityWords}
          placeholder="One word per line"
        ></textarea>
        <div class="setting-description">
          One word per line, up to {MAX_PROFANITY_WORDS} words of
          {MAX_PROFANITY_WORD_LEN} characters. Matching is per whole word and ignores
          case, so filtering “ass” leaves “class” alone.
          {chatWordsDraft.length}
          {chatWordsDraft.length === 1 ? 'word' : 'words'} will be saved.
        </div>
      </div>
      <div class="setting-group">
        <div>
          <button class="btn btn-primary" onclick={saveChatSettings} disabled={!chatDirty}>
            Save changes
          </button>
        </div>
        {#if chatFeedback}
          <div class="identity-feedback" class:error={chatFeedback.kind === 'error'}>
            {chatFeedback.text}
          </div>
        {/if}
      </div>

      <div class="setting-group">
        <span class="setting-label">Auto-moderation rules</span>
        <div class="setting-description">
          Patterns the server checks every message and every edit against, before it is
          stored or sent on. When more than one rule matches, the most severe action wins.
          Members who can manage messages are exempt, and rules never apply in an
          end-to-end encrypted channel — the server has no text to read there.
        </div>
        {#if $automodRules === null}
          <div class="setting-description">Loading…</div>
        {:else}
          {#if automodDraft.length === 0}
            <div class="setting-description">No rules yet.</div>
          {/if}
          <ul class="rule-list">
            <!-- Keyed by position: a rule has no id of its own, the
                 list is saved and stored as one, and its order is what
                 decides between two equally severe matches. -->
            {#each automodDraft as rule, index (index)}
              <li class="rule-row">
                <div class="rule-head">
                  <label class="rule-enabled">
                    <input type="checkbox" bind:checked={rule.enabled} />
                    <span>Enabled</span>
                  </label>
                  <input
                    class="rule-name"
                    type="text"
                    bind:value={rule.name}
                    maxlength={MAX_AUTOMOD_NAME_LEN}
                    placeholder="Name (shown to whoever trips it)"
                    aria-label={`Name of rule ${index + 1}`}
                  />
                  <button
                    class="btn btn-danger"
                    onclick={() => removeAutomodRule(index)}
                    aria-label={`Remove rule ${index + 1}`}
                  >
                    Remove
                  </button>
                </div>
                <div class="rule-fields">
                  <select bind:value={rule.kind} aria-label={`Match kind of rule ${index + 1}`}>
                    {#each AUTOMOD_KINDS as kind}
                      <option value={kind.id}>{kind.label}</option>
                    {/each}
                  </select>
                  <input
                    class="rule-pattern"
                    type="text"
                    spellcheck="false"
                    autocomplete="off"
                    bind:value={rule.pattern}
                    maxlength={MAX_AUTOMOD_PATTERN_LEN}
                    placeholder="Pattern"
                    aria-label={`Pattern of rule ${index + 1}`}
                  />
                  <select bind:value={rule.action} aria-label={`Action of rule ${index + 1}`}>
                    {#each AUTOMOD_ACTIONS as action}
                      <option value={action.id}>{action.label}</option>
                    {/each}
                  </select>
                  {#if rule.action === 'mute'}
                    <input
                      class="rule-mute"
                      type="number"
                      bind:value={rule.muteSeconds}
                      min={MIN_MUTE_SECONDS}
                      max={MAX_MUTE_SECONDS}
                      step="1"
                      aria-label={`Mute duration of rule ${index + 1} in seconds`}
                    />
                  {/if}
                </div>
                <div class="setting-description">
                  {AUTOMOD_KINDS.find((kind) => kind.id === rule.kind)?.description}
                  {AUTOMOD_ACTIONS.find((action) => action.id === rule.action)?.description}
                  {#if rule.action === 'mute'}
                    Muted for {describeMute(rule.muteSeconds)}.
                  {/if}
                </div>
              </li>
            {/each}
          </ul>
          <div class="rule-actions">
            <button
              class="btn"
              onclick={addAutomodRule}
              disabled={automodDraft.length >= MAX_AUTOMOD_RULES}
            >
              Add rule
            </button>
            <button
              class="btn btn-primary"
              onclick={saveAutomodRules}
              disabled={!automodDirty}
            >
              Save rules
            </button>
            <span class="setting-description">
              {automodDraft.length} of {MAX_AUTOMOD_RULES}
            </span>
          </div>
          {#if automodFeedback}
            <div class="identity-feedback" class:error={automodFeedback.kind === 'error'}>
              {automodFeedback.text}
            </div>
          {/if}
        {/if}
      </div>
    {/if}

    <div class="setting-group">
      <span class="setting-label">Ban list</span>
      <div class="setting-description">
        Everyone banned from this server. A ban follows the member's key, so it holds
        even if they come back under a different name.
      </div>
      {#if $bans === null}
        <div class="setting-description">Loading…</div>
      {:else if $bans.length === 0}
        <div class="setting-description">Nobody is banned.</div>
      {:else}
        <ul class="ban-list">
          <!-- Keyed by both fields: a ban is stored per key, but a
               row the server sent without one must not collide with
               another and break the list. -->
          {#each $bans as ban (`${ban.publicKey}:${ban.user}`)}
            <li class="ban-row">
              <span class="ban-name">{$displayNames(ban.user)}</span>
              <span class="ban-meta">
                {ban.bannedBy ? `by ${$displayNames(ban.bannedBy)} · ` : ''}{formatBanDate(
                  ban.bannedAt
                )}
              </span>
              <button class="btn" onclick={() => liftBan(ban.user)}>Unban</button>
            </li>
          {/each}
        </ul>
      {/if}
      {#if !canManageServer && chatFeedback}
        <div class="identity-feedback" class:error={chatFeedback.kind === 'error'}>
          {chatFeedback.text}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  /* Ban list, online members and the storage breakdown share the plain
     list-of-rows shape the emoji list already uses. */
  .rule-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: var(--space-3);
  }

  /* Outlined rather than filled: the inputs inside a row already carry
     `--color-surface-raised`, so a filled card would swallow them. */
  .rule-row {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-md);
  }

  .rule-head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .rule-enabled {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
    color: var(--color-on-surface-variant);
    white-space: nowrap;
  }

  .rule-enabled input[type='checkbox'] {
    accent-color: var(--color-primary);
    width: 1rem;
    height: 1rem;
    min-height: 0;
    flex-shrink: 0;
  }

  .rule-name {
    flex: 1 1 12rem;
    min-width: 0;
  }

  /* The pattern gets the room: it is the part that is read character by
     character when a rule does not do what its author expected. */
  .rule-fields {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .rule-pattern {
    flex: 1 1 14rem;
    min-width: 0;
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .rule-mute {
    flex: 0 0 7rem;
  }

  .ban-row .btn {
    margin-left: auto;
    flex-shrink: 0;
  }
</style>
