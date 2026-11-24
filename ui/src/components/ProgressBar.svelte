<script lang="ts">
  let {
    fraction,
    indeterminate = false,
    label = '',
  }: { fraction: number; indeterminate?: boolean; label?: string } = $props()

  const pct = $derived(Math.round(Math.min(1, Math.max(0, fraction)) * 100))
</script>

<div class="wrap">
  <!-- Indeterminate: no aria-valuenow, so assistive tech announces "busy", never a made-up %. -->
  <div
    class="track"
    class:indeterminate
    role="progressbar"
    aria-label={label || 'Progress'}
    aria-busy={indeterminate ? 'true' : undefined}
    aria-valuemin={indeterminate ? undefined : 0}
    aria-valuemax={indeterminate ? undefined : 100}
    aria-valuenow={indeterminate ? undefined : pct}
  >
    <div class="fill" style:width={indeterminate ? undefined : `${pct}%`}></div>
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
    position: relative;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s ease;
  }
  .indeterminate .fill {
    width: 35%;
    transition: none;
    animation: slide 1.4s ease-in-out infinite;
  }
  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(285%);
    }
  }
  /* Reduced motion: a still, full-width, dimmed bar says "working" without moving. */
  @media (prefers-reduced-motion: reduce) {
    .indeterminate .fill {
      animation: none;
      transform: none;
      width: 100%;
      opacity: 0.5;
    }
  }
  .label {
    font-size: 0.75rem;
    color: var(--muted);
  }
</style>
