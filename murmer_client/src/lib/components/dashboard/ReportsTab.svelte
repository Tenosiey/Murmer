<!--
  The message report queue: what members flagged, oldest first. The server
  sends only reports from channels this moderator manages messages in, and
  re-checks that on every dismissal. Removing the message itself happens in
  the channel, where the moderator can see it in context.
-->
<script lang="ts">
  import { untrack } from 'svelte';
  import { reports } from '#lib/stores/reports.ts';
  import { channels } from '#lib/stores/channels.ts';
  import { displayNames } from '#lib/stores/profiles.ts';
  import { t } from '#lib/i18n/index.ts';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  $effect(() => {
    if (active) untrack(() => reports.refresh());
  });

  function channelName(id: number): string {
    return $channels.find((channel) => channel.id === id)?.name ?? `#${id}`;
  }

  function formatDate(value: string | null): string {
    const parsed = value ? new Date(value) : null;
    return parsed && !Number.isNaN(parsed.getTime()) ? parsed.toLocaleString() : '';
  }
</script>

{#if active}
  <div class="settings-section">
    <h3 class="section-title">{t('reports.title')}</h3>
    <div class="setting-description">{t('reports.description')}</div>
    {#if $reports === null}
      <div class="setting-description">{t('reports.loading')}</div>
    {:else if $reports.length === 0}
      <div class="setting-description">{t('reports.empty')}</div>
    {:else}
      <ul class="report-list">
        {#each $reports as report (report.id)}
          <li class="report-row">
            <div class="report-head">
              <span class="report-author">{$displayNames(report.author)}</span>
              <span class="report-meta">
                {t('reports.meta', {
                  channel: channelName(report.channelId),
                  reporter: $displayNames(report.reporter),
                  date: formatDate(report.createdAt)
                })}
              </span>
            </div>
            <blockquote class="report-excerpt" class:muted={report.excerpt === null}>
              {#if report.deleted}
                {t('reports.deleted')}
              {:else if report.excerpt === null}
                {t('reports.sealed')}
              {:else}
                {report.excerpt}
              {/if}
            </blockquote>
            {#if report.reason}
              <div class="report-reason">{t('reports.reason', { reason: report.reason })}</div>
            {/if}
            <div class="report-actions">
              <button class="btn" onclick={() => reports.resolve(report.id)}>{t('reports.dismiss')}</button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
{/if}

<style>
  .report-list {
    list-style: none;
    margin: var(--space-3) 0 0;
    padding: 0;
    display: grid;
    gap: var(--space-3);
  }

  .report-row {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-md);
  }

  .report-head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-2);
  }

  .report-author {
    font-weight: 600;
    color: var(--color-on-surface);
  }

  .report-meta,
  .report-reason {
    font-size: var(--text-sm);
    color: var(--color-muted);
  }

  .report-excerpt {
    margin: 0;
    padding-left: var(--space-3);
    border-left: 3px solid var(--color-surface-outline);
    color: var(--color-on-surface-variant);
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }

  .report-excerpt.muted {
    font-style: italic;
    color: var(--color-muted);
  }

  .report-actions {
    display: flex;
    justify-content: flex-end;
  }
</style>
