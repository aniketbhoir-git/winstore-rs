<script lang="ts">
  import { operations } from '../lib/operations.svelte'
  import type { PackageSummary } from '../lib/types'
  import BuyInStoreButton from './BuyInStoreButton.svelte'
  import ProgressBar from './ProgressBar.svelte'

  let { pkg }: { pkg: PackageSummary } = $props()

  const op = $derived(operations.get(pkg.id))
  const running = $derived(!!op && !op.finished)
  // Installed in this session (a finished uninstall resets it).
  const installed = $derived(op?.stage === 'done' && op.operation !== 'remove')
</script>

{#if pkg.is_paid}
  <BuyInStoreButton id={pkg.id} />
{:else if running && op}
  <div class="progress">
    <ProgressBar fraction={op.fraction} label={`${op.stage}${op.message ? ': ' + op.message : ''}`} />
    <button class="secondary" onclick={() => operations.cancel(pkg.id)}>Cancel</button>
  </div>
{:else}
  <button disabled={installed} onclick={() => operations.install(pkg.id)}>
    {installed ? 'Installed' : op?.error && op.operation !== 'remove' ? 'Retry' : 'Install'}
  </button>
  {#if op?.error}
    <p class="error" role="alert">{op.error.message}</p>
  {:else if op?.stage === 'cancelled'}
    <p class="note">Cancelled</p>
  {/if}
{/if}

<style>
  button {
    padding: 0.4rem 0.9rem;
    border: 0;
    border-radius: 6px;
    font: inherit;
    font-weight: 600;
    background: var(--accent);
    color: #fff;
    cursor: pointer;
  }
  .progress button {
    background: transparent;
    color: var(--accent);
    border: 1px solid var(--accent);
  }
  button:disabled {
    opacity: 0.6;
    cursor: default;
  }
  button:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .progress {
    display: flex;
    align-items: flex-end;
    gap: 0.5rem;
  }
  .progress :global(.wrap) {
    flex: 1;
  }
  .error {
    margin: 0.3rem 0 0;
    font-size: 0.75rem;
    color: var(--danger);
  }
  .note {
    margin: 0.3rem 0 0;
    font-size: 0.75rem;
    color: var(--muted);
  }
</style>
