<script lang="ts">
  import { onMount } from 'svelte'
  import { api } from '../lib/api'

  let elevated = $state(false)

  onMount(() => {
    // A failed read shows no banner: it only informs.
    api
      .appStatus()
      .then((s) => (elevated = s.elevated))
      .catch(() => (elevated = false))
  })
</script>

{#if elevated}
  <p class="banner" role="status">
    Store runs as administrator. Installers will not ask before they change the system.
  </p>
{/if}

<style>
  .banner {
    margin: 0 0 1rem;
    padding: 0.5rem 0.75rem;
    border: 1px solid var(--accent);
    border-radius: 6px;
    font-size: 0.85rem;
  }
</style>
