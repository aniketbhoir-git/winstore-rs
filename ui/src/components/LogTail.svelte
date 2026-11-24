<script lang="ts">
  import type { LiveLine } from '../lib/operations.svelte'
  import type { OperationLogEntry } from '../lib/types'

  // Log tail pane (SPEC §6.2 step 5): saved history from the operation log plus the live
  // progress messages of operations in this session, in time order.
  let { history, live }: { history: OperationLogEntry[]; live: LiveLine[] } = $props()

  interface Row {
    key: string
    ts: number
    kind: string
    text: string
    detail: string | null
  }

  const rows = $derived<Row[]>(
    [
      ...history.map((e, i) => ({
        key: `h${e.id ?? i}`,
        ts: e.ts,
        kind: e.level,
        text: e.message,
        detail: e.detail,
      })),
      ...live.map((l, i) => ({
        key: `l${i}`,
        ts: l.ts,
        kind: l.stage,
        text: l.message,
        detail: null,
      })),
    ].sort((a, b) => a.ts - b.ts),
  )

  /** Attachment: follows the tail; re-runs whenever the row count changes. */
  function follow(node: HTMLElement) {
    void rows.length
    node.scrollTop = node.scrollHeight
  }

  function time(ts: number) {
    return new Date(ts * 1000).toLocaleTimeString()
  }
</script>

<section class="log">
  <h3>Activity</h3>
  {#if rows.length === 0}
    <p class="empty">Nothing yet. Installs and uninstalls are listed here.</p>
  {:else}
    <ol {@attach follow} aria-live="polite">
      {#each rows as row (row.key)}
        <li class={row.kind}>
          <time>{time(row.ts)}</time>
          <span class="kind">{row.kind}</span>
          <span class="text">{row.text}</span>
          {#if row.detail}<span class="detail">{row.detail}</span>{/if}
        </li>
      {/each}
    </ol>
  {/if}
</section>

<style>
  h3 {
    margin: 0 0 0.5rem;
    font-size: 1rem;
  }
  ol {
    list-style: none;
    margin: 0;
    padding: 0.5rem 0.75rem;
    max-height: 220px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    font-family: ui-monospace, 'Cascadia Mono', Consolas, monospace;
    font-size: 0.75rem;
  }
  li {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    padding: 0.1rem 0;
  }
  time,
  .kind {
    color: var(--muted);
    flex: none;
  }
  .kind {
    min-width: 6em;
  }
  .text {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .detail {
    flex-basis: 100%;
    padding-left: 1rem;
    color: var(--muted);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .warn .kind {
    color: #b7791f;
  }
  .error .kind,
  .failed .kind {
    color: var(--danger);
  }
  .empty {
    margin: 0;
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>
