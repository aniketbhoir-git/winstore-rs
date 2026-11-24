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

  it('calls start_install and shows the pending state from the reply', async () => {
    const calls = mockBackend({ start_install: () => ({ operation_id: 'op-1' }) })
    mountButton()
    await fireEvent.click(screen.getByRole('button', { name: 'Install' }))
    expect(await screen.findByRole('progressbar')).toBeTruthy()
    expect(calls).toEqual([
      { cmd: 'start_install', args: { packageId: pkg.id, options: undefined } },
    ])
  })

  it('shows the typed error of a failed command (agreements_required fixture)', async () => {
    mockBackend({
      start_install: () => {
        throw errorAgreementsRequired
      },
    })
    mountButton()
    await fireEvent.click(screen.getByRole('button', { name: 'Install' }))
    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toBe(errorAgreementsRequired.message)
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
})
