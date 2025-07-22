<script lang="ts">
  export let online: boolean | null = null;
  $: color = online === null ? '#6b7280' : online ? '#22c55e' : '#dc2626';
  $: label = online === null ? 'checking' : online ? 'online' : 'offline';
</script>

<span class="status" style="background:{color}" aria-label={label} title={label}></span>

<style>
  .status {
    width: 0.6rem;
    height: 0.6rem;
    border-radius: 50%;
    display: inline-block;
  }
</style>
