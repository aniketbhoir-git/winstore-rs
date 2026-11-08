<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'

  let version = $state('...')

  invoke<string>('app_version')
    .then((v) => (version = v))
    .catch(() => (version = 'unavailable'))
</script>

<main>
  <h1>Store</h1>
  <p>Catalog not loaded yet</p>
  <small>Core v{version}</small>
</main>

<style>
  main {
    padding: 2rem;
  }
  h1 {
    color: var(--accent);
    margin: 0 0 0.5rem;
  }
  small {
    color: #6b6b70;
  }
</style>
