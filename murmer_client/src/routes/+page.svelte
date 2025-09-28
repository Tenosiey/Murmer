<!--
  Landing route that immediately redirects the user to the login screen. This
  keeps the root URL stable while delegating all logic to the dedicated
  `/login` page.
-->
<script lang="ts">
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';

  onMount(() => {
    goto('/login');
  });
</script>

