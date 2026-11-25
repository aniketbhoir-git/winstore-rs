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

  it('keeps Cancel disabled until the start_install reply brings the operation id', async () => {
    let reply: (v: unknown) => void = () => {}
    const pending = new Promise((r) => (reply = r))
    mockBackend({ start_install: () => pending, cancel_install: () => undefined })
    mountButton()
    await fireEvent.click(screen.getByRole('button', { name: 'Install' }))
    const cancel = (await screen.findByRole('button', { name: 'Cancel' })) as HTMLButtonElement
    expect(operations.get(pkg.id)?.operationId).toBeFalsy()
    expect(cancel.disabled).toBe(true)
    reply({ operation_id: 'op-late' })
    await waitFor(() => expect(cancel.disabled).toBe(false))
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

  it('shows an indeterminate bar and the stage text for an indeterminate event', async () => {
    const calls = mockBackend({ start_install: () => ({ operation_id: 'op-1' }) })
    mountButton()
    await fireEvent.click(screen.getByRole('button', { name: 'Install' }))
    await waitFor(() => expect(calls).toHaveLength(1))
    await emit('store:progress', {
      ...eventProgress,
      indeterminate: true,
      message: 'Successfully verified installer hash',
    })
    const bar = await screen.findByRole('progressbar', { name: /verified installer hash/ })
    expect(bar.getAttribute('aria-valuenow')).toBeNull()
    expect(bar.getAttribute('aria-busy')).toBe('true')
    // A later event with a real bar returns to a measured value.
    await emit('store:progress', { ...eventProgress, indeterminate: false, fraction: 0.5 })
    const measured = await screen.findByRole('progressbar', { name: /downloading/i })
    expect(measured.getAttribute('aria-valuenow')).toBe('50')
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
      error: { ...errorAgreementsRequired, agreement_url: null, agreements: [] },
    })
    const alert = await screen.findByRole('alert')
    expect(alert.textContent?.trim()).toBe(
      'The agreement text is not available. Store cannot install this package.',
    )
    expect(alert.textContent).not.toContain(errorAgreementsRequired.message)
    expect(screen.queryByRole('dialog')).toBeNull()
    expect(screen.queryByRole('link')).toBeNull()
  })

  it('(a, b, c) an agreement with no token shows the no-text message, never a dialog', async () => {
    await startRunning()
    await emit('store:operation-finished', {
      operation_id: 'op-1',
      package_id: pkg.id,
      ok: false,
      error: { ...errorAgreementsRequired, consent_token: null },
    })
    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain('Store cannot install this package.')
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

  it('a late finished event of an earlier operation does not finish the current one', async () => {
    const calls = mockBackend({ start_install: () => ({ operation_id: 'op-B' }) })
    mountButton()
    await fireEvent.click(screen.getByRole('button', { name: 'Install' }))
    await waitFor(() => expect(calls).toHaveLength(1))
    await waitFor(() => expect(operations.get(pkg.id)?.operationId).toBe('op-B'))
    await emit('store:operation-finished', { ...eventOperationFinished, operation_id: 'op-A' })
    await tick()
    expect(operations.get(pkg.id)?.operationId).toBe('op-B')
    expect(operations.get(pkg.id)?.finished).toBe(false)
    expect(screen.getByRole('progressbar')).toBeTruthy()
    await emit('store:operation-finished', { ...eventOperationFinished, operation_id: 'op-B' })
    await screen.findByRole('button', { name: 'Installed' })
  })
})
