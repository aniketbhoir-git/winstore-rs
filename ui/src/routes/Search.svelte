<script lang="ts">
  import { onMount } from 'svelte'
  import AppCard from '../components/AppCard.svelte'
  import { api, events, toAppError } from '../lib/api'
  import { debounce } from '../lib/debounce'
  import type { AppError, PackageSummary } from '../lib/types'

  let { onopen }: { onopen?: (id: string) => void } = $props()

  const DEBOUNCE_MS = 300

  type View =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'ready'; results: PackageSummary[] }
    | { kind: 'error'; error: AppError }

  let text = $state('')
  let view = $state<View>({ kind: 'idle' })
  // Answers for any search but the latest one are ignored.
  let currentId = ''
  let counter = 0

  function showResults(id: string, results: PackageSummary[]) {
    if (id === currentId) view = { kind: 'ready', results }
  }
  function showError(id: string, error: AppError) {
    if (id === currentId) view = { kind: 'error', error }
  }

  async function run(query: string) {
    const trimmed = query.trim()
    if (!trimmed) {
      currentId = ''
      view = { kind: 'idle' }
      return
    }
    const id = `s${++counter}`
    currentId = id
    view = { kind: 'loading' }
    try {
      // Results also arrive as events; both paths are idempotent per search id.
      showResults(id, await api.searchSources(id, { text: trimmed }))
    } catch (e) {
      showError(id, toAppError(e))
    }
  }

  const debounced = debounce(run, DEBOUNCE_MS)

  function onInput() {
    if (!text.trim()) {
      debounced.cancel()
      void run('')
    } else {
      debounced(text)
    }
  }

  function onSubmit(e: SubmitEvent) {
    e.preventDefault()
    debounced.cancel()
    void run(text)
  }

  onMount(() => {
    const subs = Promise.all([
      events.onSearchResults((e) => showResults(e.search_id, e.results)),
      events.onSearchFailed((e) => showError(e.search_id, e.error)),
    ])
    return () => {
      debounced.cancel()
      void subs.then((fns) => fns.forEach((f) => f()))
    }
  })
</script>

<section>
  <form onsubmit={onSubmit} role="search">
    <input
      type="search"
      placeholder="Search apps"
      aria-label="Search apps"
      bind:value={text}
      oninput={onInput}
    />
  </form>

  <div aria-live="polite">
    {#if view.kind === 'idle'}
      <p class="state">Type a name to search the community catalog.</p>
    {:else if view.kind === 'loading'}
      <p class="state">Searching…</p>
    {:else if view.kind === 'error'}
      <div class="state error" role="alert">
        <p>{view.error.message}</p>
        {#if view.error.retryable}
          <button onclick={() => run(text)}>Try again</button>
        {/if}
      </div>
    {:else if view.results.length === 0}
      <p class="state">No apps found for “{text.trim()}”.</p>
    {:else}
      <ul class="grid">
        {#each view.results as pkg (pkg.id)}
          <li><AppCard {pkg} {onopen} /></li>
        {/each}
      </ul>
    {/if}
  </div>
</section>

<style>
  input {
    width: 100%;
    box-sizing: border-box;
    padding: 0.7rem 1rem;
    font: inherit;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    color: inherit;
  }
  input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .state {
    margin: 2rem 0;
    text-align: center;
    color: var(--muted);
  }
  .error {
    color: var(--danger);
  }
  .error button {
    padding: 0.4rem 0.9rem;
    font: inherit;
    border: 1px solid var(--danger);
    border-radius: 6px;
    background: transparent;
    color: var(--danger);
    cursor: pointer;
  }
  .grid {
    list-style: none;
    margin: 1.25rem 0 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 1rem;
  }
</style>
