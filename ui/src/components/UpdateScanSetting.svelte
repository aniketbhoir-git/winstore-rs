<script lang="ts">
  import { onMount } from 'svelte'
  import { api, toAppError } from '../lib/api'

  // SPEC §6.10: the scan runs only while Store is open; it lists updates and installs nothing.
  const MODES = [
    { value: 'off', label: 'Off' },
    { value: 'daily', label: 'Daily' },
    { value: 'weekly', label: 'Weekly' },
  ]

  let mode = $state('off')
  let error = $state<string | null>(null)

  onMount(() => {
    api
      .getUpdateScan()
      .then((m) => (mode = m))
      .catch((e) => (error = toAppError(e).message))
  })

  async function change(next: string) {
    const before = mode
    mode = next
    error = null
    try {
      await api.setUpdateScan(next)
    } catch (e) {
      mode = before
      error = toAppError(e).message
    }
  }
</script>

<section aria-label="Update check">
  <label>
    Check for updates while Store is open
    <select value={mode} onchange={(e) => change(e.currentTarget.value)}>
      {#each MODES as m (m.value)}
        <option value={m.value}>{m.label}</option>
      {/each}
    </select>
  </label>
  <small>Store only lists updates and shows a notice. It installs nothing by itself.</small>
  {#if error}<p role="alert">{error}</p>{/if}
</section>

<style>
  section {
    margin-bottom: 1rem;
  }
  small {
    display: block;
    color: var(--muted);
  }
  p {
    margin: 0.5rem 0 0;
  }
</style>
