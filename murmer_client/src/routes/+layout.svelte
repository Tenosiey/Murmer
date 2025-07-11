<script lang="ts">
  const version = '2025.7.9-alpha.1';
</script>

<slot />

<div class="version">{version}</div>

<style>
  .version {
    position: fixed;
    bottom: 0.5rem;
    right: 0.5rem;
    color: #aaa;
    font-size: 0.8rem;
  }
</style>
