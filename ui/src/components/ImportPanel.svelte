<script lang="ts">
  // Hardening design 4, import (plan step 6): reads a winget export file and shows a review
  // list. A tick per package; no "select all". Nothing installs from here yet. Everything from
  // the file is shown as text.
  import { onMount } from 'svelte'
  import { api, events, toAppError } from '../lib/api'
  import { importRequest } from '../lib/importRequest.svelte'
  import type { AppError, ImportReview, ImportRowStatus } from '../lib/types'

  type View =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'review'; review: ImportReview }
    | { kind: 'error'; error: AppError }

  let view = $state<View>({ kind: 'idle' })
  let ticked = $state<Record<string, boolean>>({})
  const count = $derived(Object.values(ticked).filter(Boolean).length)

  const STATUS_TEXT: Record<ImportRowStatus, string> = {
    ready: '',
    installed: 'Installed',
    not_valid: 'Not valid',
    source_not_supported: 'Source not supported',
  }

  async function load(path: string) {
    view = { kind: 'loading' }
    ticked = {}
    try {
      view = { kind: 'review', review: await api.readImportFile(path) }
    } catch (e) {
      view = { kind: 'error', error: toAppError(e) }
    }
  }

  onMount(() => {
    const un = events.onImportDropped((d) => void load(d.path))
    return () => void un.then((f) => f())
  })

  let lastRequest = 0
  $effect(() => {
    const { path, n } = importRequest
    if (path && n !== lastRequest) {
      lastRequest = n
      void load(path)
    }
  })
</script>

<section aria-label="Import">
  {#if view.kind === 'idle'}
    <p>Drop a winget export file on this window to review the apps in it.</p>
  {:else if view.kind === 'loading'}
    <p aria-live="polite">Reading the file…</p>
  {:else if view.kind === 'error'}
    <p role="alert">{view.error.message}</p>
  {:else}
    <h2>{view.review.file_name}</h2>
    {#if !view.review.installed_known}
      <p role="status">Could not check which apps are already installed.</p>
    {/if}
    {#if view.review.rows.length === 0}
      <p>The file lists no apps.</p>
    {:else}
      <ul>
        {#each view.review.rows as row (row.package_id + '|' + row.source)}
          <li>
            <label>
              <input
                type="checkbox"
                disabled={row.status !== 'ready'}
                checked={ticked[row.package_id] ?? false}
                onchange={(e) => (ticked[row.package_id] = e.currentTarget.checked)}
              />
              <span>{row.package_id}</span>
            </label>
            {#if row.version}<small>Version {row.version} in the file</small>{/if}
            {#if STATUS_TEXT[row.status]}<small>{STATUS_TEXT[row.status]}</small>{/if}
          </li>
        {/each}
      </ul>
      <p aria-live="polite">{count} selected. Installing from this list is not available yet.</p>
    {/if}
  {/if}
</section>

<style>
  section {
    margin-bottom: 1rem;
  }
  p,
  h2 {
    margin: 0.5rem 0 0;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0.5rem 0 0;
  }
  li small {
    margin-left: 0.5rem;
    opacity: 0.8;
  }
</style>
