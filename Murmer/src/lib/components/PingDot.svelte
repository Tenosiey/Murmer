<script lang="ts">
  export let ping: number = 0;
  $: color = ping < 100 ? '#22c55e'
    : ping < 200 ? '#eab308'
    : ping < 400 ? '#f97316'
    : '#dc2626';
</script>

<span class="ping" style="background:{color}" title={`${ping} ms`}></span>

<style>
  .ping {
    width: 0.6rem;
    height: 0.6rem;
    border-radius: 50%;
    display: inline-block;
    margin-left: 0.25rem;
  }
</style>
