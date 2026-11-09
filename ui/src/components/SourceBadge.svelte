<script lang="ts">
  import type { SourceKind } from '../lib/types'

  let { source }: { source: SourceKind } = $props()

  const labels: Record<SourceKind, string> = {
    winget_community: 'Community',
    ms_store: 'Store catalog',
    scoop: 'Scoop',
    chocolatey: 'Chocolatey',
  }
</script>

<span class="badge">{labels[source] ?? source}</span>

<style>
  .badge {
    display: inline-block;
    padding: 0.1rem 0.5rem;
    border-radius: 999px;
    font-size: 0.7rem;
    font-weight: 600;
    background: var(--badge-bg);
    color: var(--accent);
  }
</style>
