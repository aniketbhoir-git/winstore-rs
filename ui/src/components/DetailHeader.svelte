<script lang="ts">
  import type { PackageSummary } from '../lib/types'
  import RatingStars from './RatingStars.svelte'
  import SourceBadge from './SourceBadge.svelte'

  let { pkg }: { pkg: PackageSummary } = $props()

  const initial = $derived(pkg.name.trim().charAt(0).toUpperCase() || '?')
  const safeIcon = $derived(pkg.icon_url?.startsWith('https://') ? pkg.icon_url : null)
</script>

<header class="head">
  {#if safeIcon}
    <img src={safeIcon} alt="" width="88" height="88" referrerpolicy="no-referrer" />
  {:else}
    <div class="placeholder" aria-hidden="true">{initial}</div>
  {/if}
  <div class="title">
    <h2>{pkg.name}</h2>
    <span class="publisher">{pkg.publisher}</span>
    <div class="meta">
      <SourceBadge source={pkg.source} />
      {#if pkg.latest_version}<span class="version">v{pkg.latest_version}</span>{/if}
      <RatingStars rating={pkg.rating} count={pkg.rating_count} />
    </div>
  </div>
</header>

<style>
  .head {
    display: flex;
    gap: 1.25rem;
    align-items: center;
  }
  img,
  .placeholder {
    width: 88px;
    height: 88px;
    border-radius: 18px;
    flex: none;
  }
  .placeholder {
    display: grid;
    place-items: center;
    font-weight: 700;
    font-size: 2.2rem;
    color: var(--accent);
    background: var(--badge-bg);
  }
  .title {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }
  h2 {
    margin: 0;
    font-size: 1.6rem;
  }
  .publisher,
  .version {
    color: var(--muted);
    font-size: 0.85rem;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 0.6rem;
    align-items: center;
  }
</style>
