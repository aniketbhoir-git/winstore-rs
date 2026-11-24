<script lang="ts">
  import type { PackageSummary } from '../lib/types'
  import InstallButton from './InstallButton.svelte'
  import SourceBadge from './SourceBadge.svelte'

  let { pkg, onopen }: { pkg: PackageSummary; onopen?: (id: string) => void } = $props()

  const initial = $derived(pkg.name.trim().charAt(0).toUpperCase() || '?')
  const safeIcon = $derived(pkg.icon_url?.startsWith('https://') ? pkg.icon_url : null)
</script>

<article class="card">
  <div class="head">
    {#if safeIcon}
      <img src={safeIcon} alt="" width="48" height="48" referrerpolicy="no-referrer" />
    {:else}
      <div class="placeholder" aria-hidden="true">{initial}</div>
    {/if}
    <div class="title">
      <h3>
        {#if onopen}
          <button class="link" onclick={() => onopen(pkg.id)}>{pkg.name}</button>
        {:else}
          {pkg.name}
        {/if}
      </h3>
      <span class="publisher">{pkg.publisher}</span>
    </div>
  </div>
  <p class="desc">{pkg.description}</p>
  <div class="meta">
    <SourceBadge source={pkg.source} />
    {#if pkg.latest_version}<span class="version">v{pkg.latest_version}</span>{/if}
    {#if pkg.rating != null}<span class="rating">★ {pkg.rating.toFixed(1)}</span>{/if}
  </div>
  <div class="action"><InstallButton {pkg} /></div>
</article>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    padding: 1rem;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
  }
  .head {
    display: flex;
    gap: 0.75rem;
    align-items: center;
  }
  img,
  .placeholder {
    width: 48px;
    height: 48px;
    border-radius: 10px;
    flex: none;
  }
  .placeholder {
    display: grid;
    place-items: center;
    font-weight: 700;
    font-size: 1.3rem;
    color: var(--accent);
    background: var(--badge-bg);
  }
  .title {
    min-width: 0;
  }
  h3 {
    margin: 0;
    font-size: 1rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .link {
    all: unset;
    cursor: pointer;
    color: inherit;
  }
  .link:hover {
    color: var(--accent);
    text-decoration: underline;
  }
  .link:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
    border-radius: 2px;
  }
  .publisher,
  .version,
  .rating {
    font-size: 0.75rem;
    color: var(--muted);
  }
  .desc {
    margin: 0;
    font-size: 0.85rem;
    color: var(--muted);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    min-height: 2.6em;
  }
  .meta {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  .action {
    margin-top: auto;
  }
</style>
