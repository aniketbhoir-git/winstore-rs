<script lang="ts">
  import { operations } from '../lib/operations.svelte'
  import type { InstallStatus, PackageSummary } from '../lib/types'
  import BuyInStoreButton from './BuyInStoreButton.svelte'
  import ProgressBar from './ProgressBar.svelte'

  let {
    pkg,
    versions,
    status,
  }: { pkg: PackageSummary; versions: string[]; status: InstallStatus | null } = $props()

  // '' = latest.
  let version = $state('')

  const op = $derived(operations.get(pkg.id))
  const running = $derived(!!op && !op.finished)
  const ledger = $derived(status?.state ?? 'not_installed')
  const present = $derived(
    ledger === 'installed' ||
      ledger === 'update_available' ||
      ledger === 'pinned' ||
      (ledger === 'failed' && !!status?.installed_version),
  )
  const verb = $derived(op?.operation === 'remove' ? 'Uninstalling' : 'Installing')

  function install() {
    void operations.install(pkg.id, version ? { version } : undefined)
  }
</script>

<div class="actions">
  {#if pkg.is_paid}
    <BuyInStoreButton id={pkg.id} />
  {:else if running && op}
    <div class="progress">
      <ProgressBar
        fraction={op.fraction}
        label={`${verb}: ${op.stage}${op.message ? ' · ' + op.message : ''}`}
      />
      <button class="secondary" disabled={!op.operationId} onclick={() => operations.cancel(pkg.id)}>
        Cancel
      </button>
    </div>
  {:else}
    <div class="row">
      {#if present}
        <span class="installed">
          Installed{status?.installed_version ? ` v${status.installed_version}` : ''}
          {#if status?.pinned}· pinned{/if}
        </span>
        <button class="secondary" onclick={() => operations.uninstall(pkg.id)}>Uninstall</button>
      {:else}
        {#if versions.length > 1}
          <label>
            <span class="sr">Version</span>
            <select bind:value={version}>
              <option value="">Latest{pkg.latest_version ? ` (${pkg.latest_version})` : ''}</option>
              {#each versions as v (v)}<option value={v}>{v}</option>{/each}
            </select>
          </label>
        {/if}
        <button onclick={install}>{ledger === 'failed' ? 'Try again' : 'Install'}</button>
      {/if}
    </div>
    {#if op?.error}
      <p class="error" role="alert">
        {op.operation === 'remove' ? 'Uninstall' : 'Install'} did not finish: {op.error.message}
      </p>
    {:else if op?.stage === 'cancelled'}
      <p class="note">{op.operation === 'remove' ? 'Uninstall' : 'Install'} cancelled.</p>
    {/if}
  {/if}
</div>

<style>
  .actions {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    min-width: 260px;
  }
  .row,
  .progress {
    display: flex;
    gap: 0.6rem;
    align-items: center;
  }
  .progress {
    align-items: flex-end;
  }
  .progress :global(.wrap) {
    flex: 1;
  }
  button {
    padding: 0.5rem 1.2rem;
    border: 0;
    border-radius: 6px;
    font: inherit;
    font-weight: 600;
    background: var(--accent);
    color: #fff;
    cursor: pointer;
  }
  button.secondary {
    background: transparent;
    color: var(--accent);
    border: 1px solid var(--accent);
  }
  button:disabled {
    opacity: 0.6;
    cursor: default;
  }
  button:focus-visible,
  select:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  select {
    padding: 0.45rem 0.5rem;
    font: inherit;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
    color: inherit;
  }
  .installed {
    font-weight: 600;
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
  .error {
    margin: 0;
    font-size: 0.8rem;
    color: var(--danger);
  }
  .note {
    margin: 0;
    font-size: 0.8rem;
    color: var(--muted);
  }
</style>
