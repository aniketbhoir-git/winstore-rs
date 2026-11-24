// L2 tests (docs/designs/unattended-testing.md). Payloads come from the Rust fixtures.
import { tick } from 'svelte'
import { emit } from '@tauri-apps/api/event'
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { beforeAll, beforeEach, describe, expect, it } from 'vitest'
import type { PackageSummary } from '../lib/types'
import {
  errorAgreementsRequired,
  eventOperationFinished,
  eventProgress,
  installBridge,
  mockBackend,
} from '../test/ipc'

const pkg: PackageSummary = {
  id: eventProgress.package_id,
  name: 'Foo Bar',
  publisher: 'Example Publisher',
  latest_version: '1.2.3',
  description: 'A package',
  icon_url: null,
  rating: null,
  rating_count: null,
  source: 'winget_community',
  is_paid: false,
  packaging: 'msi',
}

import { operations } from '../lib/operations.svelte'
import InstallButton from './InstallButton.svelte'

const mountButton = () => render(InstallButton, { pkg })

describe('InstallButton', () => {
  beforeAll(async () => {
    installBridge()
    await operations.init()
  })
  beforeEach(() => {
    operations.byPackage = {}
    operations.lines = {}
  })

  it('calls start_install and takes the operation id from the reply', async () => {
    const calls = mockBackend({
      start_install: () => ({ operation_id: 'op-from-reply' }),
      cancel_install: () => undefined,
    })
    mountButton()
    await fireEvent.click(screen.getByRole('button', { name: 'Install' }))
    expect(await screen.findByRole('progressbar')).toBeTruthy()
    // The optimistic row has no id; only the reply can supply the one that Cancel sends.
    await waitFor(() => expect(operations.get(pkg.id)?.operationId).toBe('op-from-reply'))
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    await waitFor(() => expect(calls).toHaveLength(2))
    expect(calls).toEqual([
      { cmd: 'start_install', args: { packageId: pkg.id, options: undefined } },
      { cmd: 'cancel_install', args: { operationId: 'op-from-reply' } },
    ])
  })

  it('shows the no-text message for a failed command (agreements_required fixture) (a, b, c)', async () => {
    mockBackend({
      start_install: () => {
        throw errorAgreementsRequired
      },
    })
    mountButton()
    await fireEvent.click(screen.getByRole('button', { name: 'Install' }))
    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain('The agreement text is not available. Store cannot install this package.')
    expect(screen.getByRole('button', { name: 'Retry' })).toBeTruthy()
  })

  it('shows the result of a store:progress event', async () => {
    const calls = mockBackend({ start_install: () => ({ operation_id: 'op-1' }) })
    mountButton()
    await fireEvent.click(screen.getByRole('button', { name: 'Install' }))
    await waitFor(() => expect(calls).toHaveLength(1))
    await emit('store:progress', eventProgress)
    const bar = await screen.findByRole('progressbar', { name: /downloading/i })
    expect(bar.getAttribute('aria-valuenow')).toBe('50')
  })

  it('makes no backend call when the user clicks Installed', async () => {
    const calls = mockBackend({ start_install: () => ({ operation_id: 'op-1' }) })
    mountButton()
    await fireEvent.click(screen.getByRole('button', { name: 'Install' }))
    await waitFor(() => expect(calls).toHaveLength(1))
    await emit('store:progress', eventProgress)
    await emit('store:operation-finished', eventOperationFinished)
    const done = await screen.findByRole('button', { name: 'Installed' })
    await waitFor(() => expect((done as HTMLButtonElement).disabled).toBe(true))
    const before = calls.length
    // Element.click() honors `disabled` like a real click; a synthetic dispatch does not.
    ;(done as HTMLButtonElement).click()
    await tick()
    expect(calls.length).toBe(before)
    expect(calls.filter((c) => c.cmd === 'start_install')).toHaveLength(1)
  })

  // --- WP5 step 3 (SPEC 6.6 case (c), 6.7 gap table) ---

  const startRunning = async () => {
    const calls = mockBackend({ start_install: () => ({ operation_id: 'op-1' }) })
    mountButton()
    await fireEvent.click(screen.getByRole('button', { name: 'Install' }))
    await waitFor(() => expect(calls).toHaveLength(1))
    await emit('store:progress', eventProgress)
    await screen.findByRole('progressbar', { name: /downloading/i })
  }

  it('(a, b, c) agreements_required waits for operation-finished, then shows the no-text message and no dialog', async () => {
    await startRunning()
    // Terminal progress event first: the operation is not over, so no error and no Retry yet.
    await emit('store:progress', { ...eventProgress, stage: 'failed', fraction: 0.5 })
    await screen.findByRole('progressbar', { name: /failed/i })
    expect(screen.queryByRole('alert')).toBeNull()
    expect(screen.queryByRole('button', { name: 'Retry' })).toBeNull()

    await emit('store:operation-finished', {
      operation_id: 'op-1',
      package_id: pkg.id,
      ok: false,
      error: { ...errorAgreementsRequired, agreement_url: null },
    })
    const alert = await screen.findByRole('alert')
    expect(alert.textContent?.trim()).toBe(
      'The agreement text is not available. Store cannot install this package.',
    )
    expect(alert.textContent).not.toContain(errorAgreementsRequired.message)
    expect(screen.queryByRole('dialog')).toBeNull()
    expect(screen.queryByRole('link')).toBeNull()
  })

  it('(a, b, c) agreements_required with a link shows it as plain text, not a link or HTML', async () => {
    await startRunning()
    await emit('store:operation-finished', {
      operation_id: 'op-1',
      package_id: pkg.id,
      ok: false,
      error: { ...errorAgreementsRequired, agreement_url: 'https://example.org/terms?a=<b>x</b>' },
    })
    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain('Store cannot install this package.')
    expect(alert.textContent).toContain('https://example.org/terms?a=<b>x</b>')
    expect(alert.querySelector('a, b')).toBeNull()
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('(a, b, c) an event with cancel_failed disables Cancel; message text alone does not', async () => {
    await startRunning()
    const cancel = screen.getByRole('button', { name: 'Cancel' }) as HTMLButtonElement
    expect(cancel.disabled).toBe(false)
    await emit('store:progress', {
      ...eventProgress,
      message: 'cancel failed: the installer continues',
    })
    await screen.findByRole('progressbar', { name: /cancel failed/i })
    expect(cancel.disabled).toBe(false)
    await emit('store:progress', { ...eventProgress, cancel_failed: true })
    await waitFor(() => expect(cancel.disabled).toBe(true))
    await emit('store:progress', { ...eventProgress, fraction: 0.8 })
    await tick()
    expect(cancel.disabled).toBe(true)
  })
})
