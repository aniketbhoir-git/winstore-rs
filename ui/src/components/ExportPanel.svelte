<script lang="ts">
  import { api, toAppError } from '../lib/api'
  import { requestImport } from '../lib/importRequest.svelte'
  import type { AppError, ExportResult } from '../lib/types'

  type View =
    | { kind: 'idle' }
    | { kind: 'running' }
    | { kind: 'done'; result: ExportResult }
    | { kind: 'error'; error: AppError }

  let view = $state<View>({ kind: 'idle' })
  let folderError = $state(false)

  // Never an agreement flag from an export (hardening design 4): the terms are accepted in a terminal.
  const AGREEMENT_TEXT =
    'winget needs you to accept the terms of the winget source. Run the command winget list once in a terminal, read the terms, and accept them there.'

  // An export holds at most 500 apps and 1 MiB (SPEC 6.11); the check fails with winget_failed.
  const TOO_BIG_TEXT =
    'This device has more than 500 apps from the winget source, or the export file is larger than 1 MiB. Store can export and import at most 500 apps in one file, so nothing was saved.'

  function errorText(error: AppError): string {
    if (error.code === 'agreements_required') return AGREEMENT_TEXT
    const text = `${error.message} ${error.detail ?? ''}`
    if (error.code === 'winget_failed' && /more than 500 packages|larger than 1 MiB/.test(text))
      return TOO_BIG_TEXT
    return error.message
  }

  async function run() {
    view = { kind: 'running' }
    folderError = false
    try {
      view = { kind: 'done', result: await api.exportPackages() }
    } catch (e) {
      view = { kind: 'error', error: toAppError(e) }
    }
  }

  async function openFolder() {
    folderError = false
    try {
      await api.openExportsFolder()
    } catch {
      folderError = true
    }
  }
</script>

<section aria-label="Export">
  <button onclick={run} disabled={view.kind === 'running'}>
    {view.kind === 'running' ? 'Exporting…' : 'Export installed apps'}
  </button>
  <div aria-live="polite">
    {#if view.kind === 'done'}
      {@const exported = view.result}
      <p>
        Saved {view.result.package_count}
        {view.result.package_count === 1 ? 'app' : 'apps'} to
        <code>{view.result.path}</code>
        <button onclick={openFolder}>Open folder</button>
        <button onclick={() => requestImport(exported.path)}>Review this file</button>
      </p>
      {#if folderError}<p role="alert">The folder could not be opened.</p>{/if}
    {:else if view.kind === 'error'}
      <p role="alert">
        {errorText(view.error)}
      </p>
    {/if}
  </div>
</section>

<style>
  section {
    margin-bottom: 1rem;
  }
  p {
    margin: 0.5rem 0 0;
  }
</style>
