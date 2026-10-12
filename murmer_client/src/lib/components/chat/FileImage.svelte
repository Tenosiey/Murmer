<!--
  An uploaded image. A plain one loads straight from `/files`; a sealed one,
  from an encrypted channel, is fetched, opened with its key and shown from a
  blob, since the server only holds ciphertext. Parents size it through
  `:global(img.file-image)`.
-->
<script lang="ts">
  import { loadFile } from '#lib/upload.ts';

  interface Props {
    url: string;
    fileKey?: string;
  }

  let { url, fileKey }: Props = $props();

  let opened: string | null = $state(null);

  $effect(() => {
    if (fileKey === undefined) return;
    let src: string | null = null;
    let cancelled = false;
    loadFile(url, fileKey)
      .then((blob) => {
        if (cancelled) return;
        src = URL.createObjectURL(blob);
        opened = src;
      })
      .catch((error) => console.warn('Could not open sealed image', error));
    return () => {
      cancelled = true;
      if (src) URL.revokeObjectURL(src);
      opened = null;
    };
  });
</script>

{#if fileKey === undefined}
  <img class="file-image" src={url} alt="" loading="lazy" />
{:else if opened}
  <img class="file-image" src={opened} alt="" />
{/if}
