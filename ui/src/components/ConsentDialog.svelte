<script lang="ts">
  // SPEC §6.6 case (b): the consent dialog. Everything here is text: no raw HTML, no anchors.
  // A link opens only through `api.openAgreementLink`, which the core allows for a link of
  // this package's open agreement challenge and opens in the system browser.
  import { onMount } from 'svelte'
  import { api } from '../lib/api'
  import type { AgreementEntry, AppError } from '../lib/types'

  let {
    packageId,
    packageName,
    error,
    onaccept,
    ondecline,
  }: {
    packageId: string
    packageName: string
    error: AppError
    onaccept: () => void
    ondecline: () => void
  } = $props()

  const uid = $props.id()
  const titleId = `${uid}-title`
  const bodyId = `${uid}-body`

  const kindText = $derived(
    error.source_agreements ? 'an agreement of the winget source' : 'an agreement of the package',
  )
  // What winget printed. Older data has only the two single fields.
  const entries = $derived<AgreementEntry[]>(
    error.agreements && error.agreements.length > 0
      ? error.agreements
      : [{ label: 'Agreement', text: error.agreement_text ?? null, url: error.agreement_url ?? null }],
  )

  let dialog: HTMLDivElement | undefined = $state()
  let linkError = $state('')

  onMount(() => {
    const previous = document.activeElement as HTMLElement | null
    // Focus the dialog itself, not Accept: a screen reader reads the title and text first,
    // and a stray Enter cannot accept.
    dialog?.focus()
    return () => previous?.focus?.()
  })

  function focusable(): HTMLElement[] {
    return dialog ? Array.from(dialog.querySelectorAll<HTMLElement>('button:not([disabled])')) : []
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault()
      ondecline()
      return
    }
    if (e.key !== 'Tab') return
    // Keep Tab inside the dialog.
    const items = focusable()
    if (items.length === 0) return
    const first = items[0]
    const last = items[items.length - 1]
    const active = document.activeElement
    if (e.shiftKey && (active === first || active === dialog)) {
      e.preventDefault()
      last.focus()
    } else if (!e.shiftKey && active === last) {
      e.preventDefault()
      first.focus()
    }
  }

  async function open(url: string) {
    linkError = ''
    try {
      await api.openAgreementLink(packageId, url)
    } catch {
      linkError = 'The link could not be opened.'
    }
  }
</script>

<div class="backdrop">
  <div
    bind:this={dialog}
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby={titleId}
    aria-describedby={bodyId}
    tabindex="-1"
    {onkeydown}
  >
    <h2 id={titleId}>Agreement needed</h2>
    <div id={bodyId}>
      <p>
        {packageName}{error.version ? ` ${error.version}` : ''} needs you to accept {kindText}{error.winget_source
          ? ` (source: ${error.winget_source})`
          : ''}. Read it, then choose Accept or Cancel.
      </p>
      <ul>
        {#each entries as entry, i (i)}
          <li>
            <strong>{entry.label}</strong>
            {#if entry.text}<span class="text">{entry.text}</span>{/if}
            {#if entry.url}
              {@const url = entry.url}
              <span class="url">{url}</span>
              <button class="secondary" onclick={() => open(url)}>
                Open in browser<span class="sr"> ({entry.label})</span>
              </button>
            {/if}
          </li>
        {/each}
      </ul>
      {#if linkError}<p class="error" role="alert">{linkError}</p>{/if}
      <p class="note">Store passes your acceptance to winget for this one install only. It stores nothing.</p>
    </div>
    <div class="buttons">
      <button class="secondary" onclick={ondecline}>Cancel</button>
      <button onclick={onaccept}>Accept</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.45);
    display: grid;
    place-items: center;
    z-index: 10;
  }
  .dialog {
    background: var(--surface);
    color: inherit;
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 1.2rem 1.4rem;
    width: min(34rem, 92vw);
    max-height: 85vh;
    overflow: auto;
  }
  .dialog:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  h2 {
    margin: 0 0 0.6rem;
    font-size: 1.1rem;
  }
  ul {
    list-style: none;
    margin: 0.6rem 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
  }
  .text {
    display: block;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .url {
    display: block;
    overflow-wrap: anywhere;
    color: var(--muted);
    font-size: 0.85rem;
  }
  .note {
    color: var(--muted);
    font-size: 0.8rem;
  }
  .error {
    color: var(--danger);
    font-size: 0.8rem;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.6rem;
    margin-top: 1rem;
  }
  button {
    padding: 0.45rem 1.1rem;
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
  button:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
</style>
