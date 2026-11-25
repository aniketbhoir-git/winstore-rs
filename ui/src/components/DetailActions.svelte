<script lang="ts">
  import { agreementUrl, CANCEL_FAILED_TEXT, errorText, isValidVersion } from '../lib/errors'
  import { operations } from '../lib/operations.svelte'
  import type { InstallStatus, PackageSummary } from '../lib/types'
  import BuyInStoreButton from './BuyInStoreButton.svelte'
  import ConsentDialog from './ConsentDialog.svelte'
  import ProgressBar from './ProgressBar.svelte'

  let {
    pkg,
    versions,
    status,
    settling = false,
  }: {
    pkg: PackageSummary
    versions: string[]
    status: InstallStatus | null
    settling?: boolean
  } = $props()

  // '' = latest.
  let version = $state('')
  // '' = Default: no scope is sent and winget decides.
  let scope = $state<'' | 'user' | 'machine'>('')

  const op = $derived(operations.get(pkg.id))
  const running = $derived(!!op && !op.finished)
  // Keep the busy look until the ledger was reloaded after the operation ended.
  const busy = $derived(running || settling)
  const ledger = $derived(status?.state ?? 'not_installed')
  const present = $derived(
    ledger === 'installed' ||
      ledger === 'update_available' ||
      ledger === 'pinned' ||
      (ledger === 'failed' && !!status?.installed_version),
  )
  // The backend refuses other versions (`validate_version`); do not offer them.
  const offered = $derived(versions.filter(isValidVersion))
  const verb = $derived(op?.operation === 'remove' ? 'Uninstalling' : 'Installing')

  function install() {
    const options = { ...(version ? { version } : {}), ...(scope ? { scope } : {}) }
    void operations.install(pkg.id, Object.keys(options).length ? options : undefined)
  }
</script>

<div class="actions">
  {#if pkg.is_paid}
    <BuyInStoreButton id={pkg.id} />
  {:else if busy && op}
    <div class="progress">
      <ProgressBar
        fraction={op.fraction}
        indeterminate={op.indeterminate}
        label={`${verb}: ${op.stage}${op.message ? ' · ' + op.message : ''}`}
      />
      <button class="secondary" disabled={!op.operationId || op.cancelFailed}
        title={op.cancelFailed ? CANCEL_FAILED_TEXT : undefined}
        onclick={() => operations.cancel(pkg.id)}>
        Cancel
      </button>
    </div>
  {#if op.cancelFailed}
    <p class="note">{CANCEL_FAILED_TEXT}</p>
  {/if}
  {:else}
    <div class="row">
      {#if present}
        <span class="installed">
          Installed{status?.installed_version ? ` v${status.installed_version}` : ''}
          {#if status?.pinned}· pinned{/if}
        </span>
        <button class="secondary" onclick={() => operations.uninstall(pkg.id)}>Uninstall</button>
      {:else}
        {#if offered.length > 1}
          <label>
            <span class="sr">Version</span>
            <select bind:value={version}>
              <option value="">Latest{pkg.latest_version ? ` (${pkg.latest_version})` : ''}</option>
              {#each offered as v (v)}<option value={v}>{v}</option>{/each}
            </select>
          </label>
        {/if}
        <label>
          <span class="sr">Install scope</span>
          <select bind:value={scope}>
            <option value="">Default</option>
            <option value="user">Only for me</option>
            <option value="machine">For all users</option>
          </select>
        </label>
        <button onclick={install}>{ledger === 'failed' ? 'Try again' : 'Install'}</button>
      {/if}
    </div>
    {#if op?.error}
      <p class="error" role="alert">
        {op.operation === 'remove' ? 'Uninstall' : 'Install'} did not finish: {errorText(op.error)}
        {#if agreementUrl(op.error)}<span class="url">{agreementUrl(op.error)}</span>{/if}
      </p>
    {:else if op?.stage === 'cancelled'}
      <p class="note">{op.operation === 'remove' ? 'Uninstall' : 'Install'} cancelled.</p>
    {/if}
  {/if}
</div>

{#if op?.agreement}
  <ConsentDialog
    packageId={pkg.id}
    packageName={pkg.name}
    error={op.agreement}
    onaccept={() => operations.acceptAgreement(pkg.id)}
    ondecline={() => operations.declineAgreement(pkg.id)}
  />
{/if}

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
  .url {
    display: block;
    overflow-wrap: anywhere;
  }
  .note {
    margin: 0;
    font-size: 0.8rem;
    color: var(--muted);
  }
</style>
