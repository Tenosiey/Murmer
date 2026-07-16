<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import { browser } from '$app/environment';
  import { fetchLinkPreview, giphyGifUrl, type LinkPreviewData } from '$lib/link-preview';

  interface Props {
    url: string;
  }

  let { url }: Props = $props();

  const PREVIEW_TIMEOUT_MS = 6000;

  let safeUrl: URL | null = $state(null);
  let displayHost = $state('');
  let metadataTimeout: number | null = null;
  let metadataAbort: AbortController | null = null;
  let currentUrl = $state('');

  let giphyGif: string | null = $state(null);

  let youtubeId: string | null = $state(null);
  let youtubeTitle = $state('');
  let youtubeAuthor = $state('');
  let youtubeThumbnail = $state('');
  let youtubeError = $state(false);

  let preview: LinkPreviewData | null = $state(null);
  let previewImageFailed = $state(false);


  function parseUrl() {
    try {
      const parsed = new URL(url);
      if (parsed.protocol === 'http:' || parsed.protocol === 'https:') {
        safeUrl = parsed;
        currentUrl = parsed.toString();
        displayHost = parsed.hostname.replace(/^www\./, '');
      } else {
        safeUrl = null;
        currentUrl = '';
        displayHost = '';
      }
    } catch (error) {
      safeUrl = null;
      currentUrl = '';
      displayHost = '';
    }
  }

  function extractYouTubeId(target: URL | null): string | null {
    if (!target) return null;
    const host = target.hostname.toLowerCase();
    if (host.includes('youtube.com')) {
      const searchParams = target.searchParams.get('v');
      if (searchParams) return searchParams;
      const pathname = target.pathname.split('/');
      const last = pathname[pathname.length - 1];
      if (pathname.includes('embed') && last) {
        return last;
      }
    }
    if (host === 'youtu.be') {
      const id = target.pathname.replace(/^\//, '');
      return id || null;
    }
    return null;
  }

  async function loadYoutubeMetadata(videoId: string, targetUrl: string) {
    youtubeThumbnail = `https://img.youtube.com/vi/${videoId}/hqdefault.jpg`;
    if (!browser) return;
    try {
      if (metadataAbort) {
        metadataAbort.abort();
      }
      const controller = new AbortController();
      metadataAbort = controller;
      metadataTimeout = window.setTimeout(() => controller.abort(), PREVIEW_TIMEOUT_MS);
      const response = await fetch(
        `https://www.youtube.com/oembed?format=json&url=${encodeURIComponent(targetUrl)}`,
        { signal: controller.signal }
      );
      if (metadataTimeout) {
        clearTimeout(metadataTimeout);
        metadataTimeout = null;
      }
      metadataAbort = null;
      if (!response.ok) {
        youtubeError = true;
        return;
      }
      const data = await response.json();
      youtubeTitle = typeof data.title === 'string' ? data.title : '';
      youtubeAuthor = typeof data.author_name === 'string' ? data.author_name : '';
      if (typeof data.thumbnail_url === 'string' && data.thumbnail_url) {
        youtubeThumbnail = data.thumbnail_url;
      }
    } catch (error) {
      if (metadataTimeout) {
        clearTimeout(metadataTimeout);
        metadataTimeout = null;
      }
      if ((error as DOMException)?.name === 'AbortError') {
        youtubeError = false;
      } else {
        youtubeError = true;
      }
      metadataAbort = null;
    }
  }

  async function loadPreview(targetUrl: string) {
    const data = await fetchLinkPreview(targetUrl);
    // Ignore stale responses after the url prop changed.
    if (targetUrl === currentUrl) {
      preview = data;
    }
  }

  /* When the GIF fails to load, fall back to the regular metadata card. */
  function handleGiphyError() {
    giphyGif = null;
    if (browser && safeUrl) loadPreview(currentUrl);
  }

  function setupPreview() {
    giphyGif = safeUrl ? giphyGifUrl(currentUrl) : null;
    youtubeId = giphyGif ? null : extractYouTubeId(safeUrl);
    youtubeTitle = '';
    youtubeAuthor = '';
    youtubeError = false;
    preview = null;
    previewImageFailed = false;
    if (metadataTimeout) {
      clearTimeout(metadataTimeout);
      metadataTimeout = null;
    }
    if (metadataAbort) {
      metadataAbort.abort();
      metadataAbort = null;
    }
    if (!browser || !safeUrl) return;
    if (giphyGif) return;
    if (youtubeId) {
      loadYoutubeMetadata(youtubeId, currentUrl);
    } else {
      loadPreview(currentUrl);
    }
  }

  onDestroy(() => {
    if (metadataTimeout) clearTimeout(metadataTimeout);
    if (metadataAbort) metadataAbort.abort();
  });
  /* Re-parse and rebuild the preview whenever the url prop changes; untrack
     keeps the intermediate state writes from becoming effect dependencies. */
  $effect(() => {
    void url;
    untrack(() => {
      parseUrl();
      setupPreview();
    });
  });
</script>

{#if safeUrl}
  {#if giphyGif}
    <a
      class="link-preview giphy"
      href={currentUrl}
      target="_blank"
      rel="noopener noreferrer"
      aria-label={`Open GIF on ${displayHost}`}
    >
      <img src={giphyGif} alt="GIF" loading="lazy" onerror={handleGiphyError} />
      <span class="giphy-badge" aria-hidden="true">GIF</span>
    </a>
  {:else if youtubeId}
    <div class="link-preview youtube">
      <a href={currentUrl} target="_blank" rel="noopener noreferrer" class="youtube-thumb" aria-label={`Open video on ${displayHost}`}>
        {#if youtubeThumbnail}
          <img src={youtubeThumbnail} alt={youtubeTitle ? `${youtubeTitle} thumbnail` : 'YouTube thumbnail'} loading="lazy" />
        {/if}
        <span class="youtube-icon" aria-hidden="true">▶</span>
      </a>
      <div class="youtube-meta">
        <span class="youtube-label">YouTube</span>
        {#if youtubeTitle}
          <p class="youtube-title">{youtubeTitle}</p>
        {/if}
        {#if youtubeAuthor}
          <p class="youtube-author">{youtubeAuthor}</p>
        {/if}
        {#if youtubeError}
          <p class="youtube-error">Preview limited – open the link to view details.</p>
        {/if}
      </div>
    </div>
  {:else if preview}
    <a class="link-preview card" href={currentUrl} target="_blank" rel="noopener noreferrer">
      <div class="card-body">
        <span class="card-site">{preview.siteName ?? displayHost}</span>
        {#if preview.title}
          <p class="card-title">{preview.title}</p>
        {/if}
        {#if preview.description}
          <p class="card-description">{preview.description}</p>
        {/if}
      </div>
      {#if preview.image && !previewImageFailed}
        <img
          class="card-image"
          src={preview.image}
          alt=""
          loading="lazy"
          onerror={() => (previewImageFailed = true)}
        />
      {/if}
    </a>
  {/if}
{/if}

<style>
  .link-preview {
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-md);
    background: var(--color-surface-raised);
    overflow: hidden;
    max-width: 460px;
    box-shadow: var(--shadow-xs);
    transition:
      border-color var(--motion-duration-short) var(--motion-easing-standard),
      box-shadow var(--motion-duration-short) var(--motion-easing-standard);
  }

  .link-preview:hover {
    border-color: var(--color-outline-strong);
    box-shadow: var(--shadow-sm);
  }

  .card {
    display: flex;
    gap: var(--space-3);
    padding: var(--space-3);
    text-decoration: none;
    color: inherit;
    align-items: flex-start;
  }

  .card-body {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
    flex: 1;
  }

  .card-site {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--color-muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .card-title {
    margin: 0;
    font-weight: 600;
    font-size: var(--text-md);
    line-height: 1.3;
    color: var(--color-secondary);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .card:hover .card-title,
  .card:focus-visible .card-title {
    text-decoration: underline;
  }

  .card-description {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--color-on-surface-variant);
    line-height: 1.4;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .card-image {
    width: 5.5rem;
    height: 5.5rem;
    object-fit: cover;
    border-radius: var(--radius-sm);
    flex-shrink: 0;
  }

  .giphy {
    position: relative;
    display: block;
    max-width: 320px;
  }

  .giphy img {
    display: block;
    width: 100%;
    height: auto;
  }

  .giphy-badge {
    position: absolute;
    bottom: var(--space-2);
    left: var(--space-2);
    padding: 0 var(--space-1);
    border-radius: var(--radius-xs);
    font-size: 0.625rem;
    font-weight: 700;
    letter-spacing: 0.04em;
    line-height: 1rem;
    background: rgba(0, 0, 0, 0.65);
    color: white;
    opacity: 0;
    transition: opacity var(--motion-duration-short) var(--motion-easing-standard);
  }

  .giphy:hover .giphy-badge,
  .giphy:focus-visible .giphy-badge {
    opacity: 1;
  }

  .youtube {
    display: flex;
    gap: var(--space-3);
    padding: var(--space-3);
  }

  .youtube-thumb {
    position: relative;
    width: 160px;
    flex-shrink: 0;
    border-radius: var(--radius-sm);
    overflow: hidden;
    display: block;
  }

  .youtube-thumb:hover .youtube-icon {
    background: rgba(0, 0, 0, 0.8);
  }

  .youtube-thumb img {
    width: 100%;
    height: auto;
    display: block;
  }

  .youtube-icon {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    background: rgba(0, 0, 0, 0.65);
    backdrop-filter: blur(8px);
    border-radius: 50%;
    width: 44px;
    height: 44px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: white;
    font-size: var(--text-xl);
    transition: all var(--motion-duration-short) var(--motion-easing-standard);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
  }

  .youtube-meta {
    display: flex;
    flex-direction: column;
    justify-content: center;
    font-size: var(--text-sm);
    gap: var(--space-1);
  }

  .youtube-label {
    font-weight: 700;
    color: var(--color-error);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: var(--text-xs);
  }

  .youtube-title {
    margin: 0;
    font-weight: 600;
    line-height: 1.3;
    color: var(--color-on-surface);
    font-size: var(--text-md);
  }

  .youtube-author {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-sm);
  }

  .youtube-error {
    margin: 0;
    color: var(--color-muted);
    font-style: italic;
    font-size: var(--text-sm);
  }
</style>
