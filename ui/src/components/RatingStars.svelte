<script lang="ts">
  let { rating, count = null }: { rating: number | null; count?: number | null } = $props()

  const value = $derived(rating == null ? null : Math.min(5, Math.max(0, rating)))
  const full = $derived(value == null ? 0 : Math.round(value))
  const label = $derived(
    value == null
      ? 'No rating yet'
      : `Rated ${value.toFixed(1)} out of 5${count != null ? ` from ${count} ratings` : ''}`,
  )
</script>

<span class="rating" role="img" aria-label={label}>
  {#if value == null}
    <span class="none">No rating yet</span>
  {:else}
    <span class="stars" aria-hidden="true">
      {#each [1, 2, 3, 4, 5] as i (i)}<span class={{ on: i <= full }}>★</span>{/each}
    </span>
    <span class="num" aria-hidden="true">{value.toFixed(1)}</span>
    {#if count != null}<span class="count" aria-hidden="true">({count})</span>{/if}
  {/if}
</span>

<style>
  .rating {
    display: inline-flex;
    gap: 0.35rem;
    align-items: baseline;
    font-size: 0.85rem;
    color: var(--muted);
  }
  .stars span {
    color: var(--track);
  }
  .stars span.on {
    color: var(--accent);
  }
  .num {
    font-weight: 600;
    color: var(--text);
  }
</style>
