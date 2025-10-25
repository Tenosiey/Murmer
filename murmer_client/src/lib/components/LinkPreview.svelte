<script lang="ts">
  import { onDestroy } from 'svelte';
  import { browser } from '$app/environment';

  export let url: string;

  const PREVIEW_TIMEOUT_MS = 6000;

  let safeUrl: URL | null = null;
  let displayHost = '';
  let iframeTimedOut = false;
  let iframeTimeout: number | null = null;
  let metadataTimeout: number | null = null;
  let metadataAbort: AbortController | null = null;
  let currentUrl = '';

  let youtubeId: string | null = null;
  let youtubeTitle = '';
  let youtubeAuthor = '';
  let youtubeThumbnail = '';
  let youtubeError = false;

  $: parseUrl();
  $: setupYoutube();

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

  function setupYoutube() {
    youtubeId = extractYouTubeId(safeUrl);
    youtubeTitle = '';
    youtubeAuthor = '';
    youtubeError = false;
    if (metadataTimeout) {
      clearTimeout(metadataTimeout);
      metadataTimeout = null;
    }
    if (iframeTimeout) {
      clearTimeout(iframeTimeout);
      iframeTimeout = null;
    }
    if (metadataAbort) {
      metadataAbort.abort();
      metadataAbort = null;
    }
    iframeTimedOut = false;
    if (youtubeId && browser) {
      loadYoutubeMetadata(youtubeId, currentUrl);
    }
  }

  function startIframeTimeout() {
    if (!browser) return;
    if (iframeTimeout) {
      clearTimeout(iframeTimeout);
    }
    iframeTimedOut = false;
    iframeTimeout = window.setTimeout(() => {
      iframeTimedOut = true;
      iframeTimeout = null;
    }, PREVIEW_TIMEOUT_MS);
  }

  function handleIframeLoad() {
    if (iframeTimeout) {
      clearTimeout(iframeTimeout);
      iframeTimeout = null;
    }
  }

  onDestroy(() => {
    if (iframeTimeout) clearTimeout(iframeTimeout);
    if (metadataTimeout) clearTimeout(metadataTimeout);
    if (metadataAbort) metadataAbort.abort();
  });

  $: if (browser && safeUrl && !youtubeId) {
    startIframeTimeout();
  }
</script>

{#if safeUrl}
  {#if youtubeId}
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
  {:else}
    <div class="link-preview">
      <div class="preview-header">
        <span class="host">{displayHost}</span>
        <a href={currentUrl} target="_blank" rel="noopener noreferrer">Open</a>
      </div>
      {#if !iframeTimedOut}
        <iframe
          title={`Preview of ${displayHost}`}
          src={currentUrl}
          loading="lazy"
          sandbox="allow-same-origin"
          on:load={handleIframeLoad}
        ></iframe>
      {:else}
        <div class="preview-fallback">
          <p>Preview unavailable. <a href={currentUrl} target="_blank" rel="noopener noreferrer">Open link</a>.</p>
        </div>
      {/if}
    </div>
  {/if}
{/if}

<style>
  .link-preview {
    border: 1px solid var(--border-color, rgba(255, 255, 255, 0.08));
    border-radius: 8px;
    background: var(--panel-color, rgba(255, 255, 255, 0.03));
    overflow: hidden;
    max-width: 420px;
  }

  .link-preview iframe {
    width: 100%;
    height: 200px;
    border: none;
    background: transparent;
  }

  .preview-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.5rem 0.75rem;
    font-size: 0.85rem;
    font-weight: 500;
    background: var(--panel-header, rgba(255, 255, 255, 0.05));
  }

  .preview-header a {
    color: var(--accent-color, #4ba3ff);
    text-decoration: none;
    font-weight: 600;
  }

  .preview-header a:hover,
  .preview-header a:focus-visible {
    text-decoration: underline;
  }

  .preview-fallback {
    padding: 0.75rem;
    font-size: 0.85rem;
    color: var(--muted-color, rgba(255, 255, 255, 0.7));
  }

  .preview-fallback a {
    color: var(--accent-color, #4ba3ff);
    text-decoration: none;
  }

  .preview-fallback a:hover,
  .preview-fallback a:focus-visible {
    text-decoration: underline;
  }

  .youtube {
    display: flex;
    gap: 0.75rem;
    padding: 0.75rem;
  }

  .youtube-thumb {
    position: relative;
    width: 150px;
    flex-shrink: 0;
    border-radius: 6px;
    overflow: hidden;
    display: block;
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
    background: rgba(0, 0, 0, 0.6);
    border-radius: 50%;
    width: 40px;
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: white;
    font-size: 1.2rem;
  }

  .youtube-meta {
    display: flex;
    flex-direction: column;
    justify-content: center;
    font-size: 0.85rem;
    gap: 0.25rem;
  }

  .youtube-label {
    font-weight: 600;
    color: var(--accent-color, #ff4b4b);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .youtube-title {
    margin: 0;
    font-weight: 600;
    line-height: 1.2;
  }

  .youtube-author {
    margin: 0;
    color: var(--muted-color, rgba(255, 255, 255, 0.75));
  }

  .youtube-error {
    margin: 0;
    color: var(--muted-color, rgba(255, 255, 255, 0.75));
    font-style: italic;
  }
</style>
