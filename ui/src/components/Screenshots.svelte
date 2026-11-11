<script lang="ts">
  let { urls, name }: { urls: string[]; name: string } = $props()

  // Only HTTPS images from the catalog; anything else is dropped.
  const safe = $derived(urls.filter((u) => u.startsWith('https://')))
</script>

{#if safe.length > 0}
  <ul class="strip" aria-label="Screenshots">
    {#each safe as url, i (url)}
      <li>
        <img
          src={url}
          alt={`${name} screenshot ${i + 1}`}
          loading="lazy"
          referrerpolicy="no-referrer"
        />
      </li>
    {/each}
  </ul>
{/if}

<style>
  .strip {
    list-style: none;
    margin: 0;
    padding: 0 0 0.5rem;
    display: flex;
    gap: 0.75rem;
    overflow-x: auto;
  }
  li {
    flex: none;
  }
  img {
    display: block;
    height: 220px;
    max-width: 420px;
    object-fit: cover;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--surface);
  }
</style>
