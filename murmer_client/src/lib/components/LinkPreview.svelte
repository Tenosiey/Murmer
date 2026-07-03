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

  .link-preview iframe {
    width: 100%;
    height: 220px;
    border: none;
    background: color-mix(in srgb, var(--color-surface) 95%, transparent);
  }

  .preview-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.7rem 1rem;
    font-size: var(--text-sm);
    font-weight: 600;
    background: color-mix(in srgb, var(--color-surface-elevated) 90%, transparent);
    border-bottom: 1px solid var(--color-surface-outline);
  }

  .preview-header .host {
    color: var(--color-on-surface-variant);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .preview-header a {
    color: var(--color-secondary);
    text-decoration: none;
    font-weight: 600;
    transition: color var(--motion-duration-short) var(--motion-easing-standard);
  }

  .preview-header a:hover,
  .preview-header a:focus-visible {
    color: var(--color-primary);
    text-decoration: underline;
  }

  .preview-fallback {
    padding: 1.2rem;
    font-size: var(--text-md);
    color: var(--color-muted);
    text-align: center;
  }

  .preview-fallback a {
    color: var(--color-secondary);
    text-decoration: none;
    font-weight: 600;
  }

  .preview-fallback a:hover,
  .preview-fallback a:focus-visible {
    color: var(--color-primary);
    text-decoration: underline;
  }

  .youtube {
    display: flex;
    gap: 1rem;
    padding: 1rem;
  }

  .youtube-thumb {
    position: relative;
    width: 160px;
    flex-shrink: 0;
    border-radius: var(--radius-sm);
    overflow: hidden;
    display: block;
    transition: transform var(--motion-duration-short) var(--motion-easing-standard);
  }

  .youtube-thumb:hover {
    transform: scale(1.02);
  }

  .youtube-thumb:hover .youtube-icon {
    transform: translate(-50%, -50%) scale(1.1);
    background: rgba(0, 0, 0, 0.75);
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
    gap: 0.4rem;
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
