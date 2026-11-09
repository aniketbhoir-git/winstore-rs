<script lang="ts">
  import { api, toAppError } from '../lib/api'
  import type { PackageId } from '../lib/types'

  // SPEC §0 rule 1 / §6.4: the only action for a paid item. Opens the official product page
  // in the default browser; nothing else happens in-app.
  let { id }: { id: PackageId } = $props()

  let error = $state<string | null>(null)

  async function open() {
    error = null
    try {
      // Product id = the source-local part of the package id.
      await api.openStorePage(id.slice(id.indexOf('/') + 1))
    } catch (e) {
      error = toAppError(e).message
    }
  }
</script>

<button class="secondary" onclick={open}>Buy in Microsoft Store</button>
{#if error}<p class="error" role="alert">{error}</p>{/if}

<style>
  button {
    padding: 0.4rem 0.9rem;
    border-radius: 6px;
    font: inherit;
    font-weight: 600;
    cursor: pointer;
    background: transparent;
    color: var(--accent);
    border: 1px solid var(--accent);
  }
  button:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .error {
    margin: 0.3rem 0 0;
    font-size: 0.75rem;
    color: var(--danger);
  }
</style>
