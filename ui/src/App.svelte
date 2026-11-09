<script lang="ts">
  import { onMount } from 'svelte'
  import { api } from './lib/api'
  import { operations } from './lib/operations.svelte'
  import AppDetail from './routes/AppDetail.svelte'
  import Search from './routes/Search.svelte'

  let version = $state('...')
  // Package shown on the detail page; null = search. Search stays mounted to keep its results.
  let selected = $state<string | null>(null)

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
  <div hidden={selected !== null}>
    <Search onopen={(id) => (selected = id)} />
  </div>
  {#if selected}
    {#key selected}
      <AppDetail id={selected} onback={() => (selected = null)} />
    {/key}
  {/if}
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
