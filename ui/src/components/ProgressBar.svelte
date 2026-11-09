<script lang="ts">
  let { fraction, label = '' }: { fraction: number; label?: string } = $props()

  const pct = $derived(Math.round(Math.min(1, Math.max(0, fraction)) * 100))
</script>

<div class="wrap">
  <div
    class="track"
    role="progressbar"
    aria-valuemin="0"
    aria-valuemax="100"
    aria-valuenow={pct}
    aria-label={label || 'Progress'}
  >
    <div class="fill" style:width="{pct}%"></div>
  </div>
  {#if label}<span class="label">{label}</span>{/if}
</div>

<style>
  .wrap {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  .track {
    height: 6px;
    border-radius: 3px;
    background: var(--track);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s ease;
  }
  .label {
    font-size: 0.75rem;
    color: var(--muted);
  }
</style>
