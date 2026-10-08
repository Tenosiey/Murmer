<!--
  Inline player for an audio attachment or voice message. The clip is fetched
  into a blob only when the play button is pressed: the app's CSP admits media
  from `blob:` and not from the server's origin, and fetching every clip in
  the history just to render it would download all of them up front.
-->
<script lang="ts">
  import { t } from '$lib/i18n';
  interface Props {
    url: string;
    name: string;
  }

  let { url, name }: Props = $props();

  let src: string | null = $state(null);
  let loading = $state(false);
  let failed = $state(false);

  async function load() {
    loading = true;
    failed = false;
    try {
      const response = await fetch(url);
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      src = URL.createObjectURL(await response.blob());
    } catch (error) {
      console.warn('Could not load audio attachment', error);
      failed = true;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    const current = src;
    return () => {
      if (current) URL.revokeObjectURL(current);
    };
  });
</script>

{#if src}
  <!-- svelte-ignore a11y_media_has_caption -->
  <audio class="audio-player" controls autoplay {src} aria-label={name}></audio>
{:else}
  <button type="button" class="btn audio-load" onclick={load} disabled={loading}>
    <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M8 5v14l11-7z" /></svg>
    {t(loading ? 'audioAttachment.loading' : failed ? 'audioAttachment.failed' : 'audioAttachment.play')}
  </button>
{/if}

<style>
  .audio-player {
    display: block;
    margin-top: var(--space-2);
    width: min(360px, 100%);
  }

  .audio-load {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    margin-top: var(--space-2);
  }
</style>
