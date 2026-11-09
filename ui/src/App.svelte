<script lang="ts">
  import { onMount } from 'svelte'
  import { api } from './lib/api'
  import { operations } from './lib/operations.svelte'
  import Search from './routes/Search.svelte'

  let version = $state('...')

  onMount(() => {
    void operations.init()
    api
      .appVersion()
      .then((v) => (version = v))
      .catch(() => (version = 'unavailable'))
  })
</script>

<main>
  <header>
    <h1>Store</h1>
    <small>Core v{version}</small>
  </header>
  <Search />
</main>

<style>
  main {
    max-width: 1100px;
    margin: 0 auto;
    padding: 1.5rem 2rem;
  }
  header {
    display: flex;
    align-items: baseline;
    gap: 0.75rem;
    margin-bottom: 1rem;
  }
  h1 {
    color: var(--accent);
    margin: 0;
  }
  small {
    color: var(--muted);
  }
</style>
