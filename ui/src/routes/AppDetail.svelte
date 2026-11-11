<script lang="ts">
  import { onMount } from 'svelte'
  import DetailActions from '../components/DetailActions.svelte'
  import DetailHeader from '../components/DetailHeader.svelte'
  import LogTail from '../components/LogTail.svelte'
  import Screenshots from '../components/Screenshots.svelte'
  import { api, events, toAppError } from '../lib/api'
  import { operations } from '../lib/operations.svelte'
  import type { AppError, InstallStatus, OperationLogEntry, PackageDetail } from '../lib/types'

  let { id, onback }: { id: string; onback: () => void } = $props()

  const LOG_LIMIT = 100

  let detail = $state<PackageDetail | null>(null)
  let loadError = $state<AppError | null>(null)
  let status = $state<InstallStatus | null>(null)
  let history = $state<OperationLogEntry[]>([])

  const versions = $derived(
    detail
      ? Object.keys(detail.versions).sort((a, b) =>
          b.localeCompare(a, undefined, { numeric: true }),
        )
      : [],
  )
  const live = $derived(operations.lines[id] ?? [])
  const size = $derived(
    detail?.installer.size_bytes != null
      ? `${(detail.installer.size_bytes / 1048576).toFixed(1)} MB`
      : null,
  )

  async function loadDetail() {
    loadError = null
    try {
      detail = await api.getAppDetail(id)
    } catch (e) {
      loadError = toAppError(e)
    }
  }

  /** Ledger state and saved log; refreshed whenever an operation for this app ends. */
  async function refresh() {
    try {
      status = await api.getInstallStatus(id)
      if (status.active) operations.adopt(id, status.active)
    } catch {
      // Keep the last known state; the actions still work.
    }
    try {
      history = await api.getOperationLog(id, LOG_LIMIT)
    } catch {
      history = []
    }
  }

  onMount(() => {
    void loadDetail()
    void refresh()
    const sub = events.onOperationFinished((e) => {
      if (e.package_id === id) void refresh()
    })
    return () => void sub.then((f) => f())
  })
</script>

<article class="detail">
  <button class="back" onclick={onback}>← Back to results</button>

  {#if loadError}
    <div class="state error" role="alert">
      <p>{loadError.message}</p>
      <button class="retry" onclick={loadDetail}>Try again</button>
    </div>
  {:else if !detail}
    <p class="state">Loading…</p>
  {:else}
    <div class="top">
      <DetailHeader pkg={detail.summary} />
      <DetailActions pkg={detail.summary} {versions} {status} />
    </div>

    <Screenshots urls={detail.screenshots} name={detail.summary.name} />

    <section>
      <h3>Description</h3>
      <p class="text">{detail.full_description || detail.summary.description}</p>
    </section>

    {#if detail.release_notes}
      <section>
        <h3>Release notes</h3>
        <p class="text">{detail.release_notes}</p>
      </section>
    {/if}

    <div class="cols">
      <section>
        <h3>Information</h3>
        <dl>
          <dt>Publisher</dt>
          <dd>{detail.summary.publisher}</dd>
          {#if detail.license}<dt>License</dt><dd>{detail.license}</dd>{/if}
          {#if detail.website}<dt>Website</dt><dd class="url">{detail.website}</dd>{/if}
          <dt>Installer</dt>
          <dd>
            {detail.installer.install_type}{detail.installer.architecture
              ? ` · ${detail.installer.architecture}`
              : ''}{size ? ` · ${size}` : ''}
          </dd>
          <dt>Package id</dt>
          <dd class="url">{detail.summary.id}</dd>
        </dl>
      </section>

      <section>
        <h3>Versions</h3>
        {#if versions.length === 0}
          <p class="muted">Only the latest version is listed.</p>
        {:else}
          <ul class="versions">
            {#each versions as v (v)}
              <li>
                <span>{v}</span>
                {#if detail.versions[v].release_date}
                  <span class="muted">{detail.versions[v].release_date}</span>
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    </div>

    <LogTail {history} {live} />
  {/if}
</article>

<style>
  .detail {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
  }
  .back,
  .retry {
    align-self: flex-start;
    padding: 0.3rem 0.7rem;
    font: inherit;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
    color: inherit;
    cursor: pointer;
  }
  .back:focus-visible,
  .retry:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .top {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 1.5rem;
  }
  h3 {
    margin: 0 0 0.5rem;
    font-size: 1rem;
  }
  .text {
    margin: 0;
    white-space: pre-line;
    line-height: 1.5;
  }
  .cols {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: 1.5rem;
  }
  dl {
    margin: 0;
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.35rem 1rem;
    font-size: 0.9rem;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    min-width: 0;
  }
  .url {
    overflow-wrap: anywhere;
    user-select: all;
  }
  .versions {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 200px;
    overflow-y: auto;
    font-size: 0.9rem;
  }
  .versions li {
    display: flex;
    justify-content: space-between;
    padding: 0.2rem 0;
    border-bottom: 1px solid var(--border);
  }
  .muted {
    color: var(--muted);
  }
  .state {
    margin: 2rem 0;
    text-align: center;
    color: var(--muted);
  }
  .error {
    color: var(--danger);
  }
</style>
