// The version list offers only what the backend `validate_version` accepts.
import { emit } from '@tauri-apps/api/event'
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { beforeAll, describe, expect, it } from 'vitest'
import type { PackageSummary } from '../lib/types'
import { isValidVersion } from '../lib/errors'
import { operations } from '../lib/operations.svelte'
import { eventProgress, installBridge, mockBackend } from '../test/ipc'
import DetailActions from './DetailActions.svelte'

const pkg: PackageSummary = {
  id: eventProgress.package_id,
  name: 'Foo',
  publisher: 'P',
  latest_version: '2.0',
  description: '',
  icon_url: null,
  rating: null,
  rating_count: null,
  source: 'winget_community',
  is_paid: false,
  packaging: 'msi',
}

describe('version rule', () => {
  beforeAll(async () => {
    installBridge()
    await operations.init()
  })

  it('matches validate_version', () => {
    expect(isValidVersion('1.2.3-beta+5_x')).toBe(true)
    expect(isValidVersion('')).toBe(false)
    expect(isValidVersion('-1.0')).toBe(false)
    expect(isValidVersion('1 0')).toBe(false)
    expect(isValidVersion('1.0;rm')).toBe(false)
    expect(isValidVersion('é1')).toBe(false)
    expect(isValidVersion('1'.repeat(64))).toBe(true)
    expect(isValidVersion('1'.repeat(65))).toBe(false)
  })

  it('lists only versions the backend accepts', () => {
    mockBackend({})
    render(DetailActions, {
      pkg,
      versions: ['2.0', '1.0', '-bad', '1 0', 'x'.repeat(65)],
      status: null,
    })
    const options = screen.getAllByRole('option').map((o) => o.getAttribute('value'))
    expect(options).toEqual(['', '2.0', '1.0'])
  })

  it('(a, b, c) an event with cancel_failed disables Cancel', async () => {
    operations.byPackage = {}
    const calls = mockBackend({ start_install: () => ({ operation_id: 'op-1' }) })
    render(DetailActions, { pkg, versions: [], status: null })
    await fireEvent.click(screen.getByRole('button', { name: 'Install' }))
    await waitFor(() => expect(calls).toHaveLength(1))
    await emit('store:progress', eventProgress)
    const cancel = (await screen.findByRole('button', { name: 'Cancel' })) as HTMLButtonElement
    await waitFor(() => expect(cancel.disabled).toBe(false))
    await emit('store:progress', { ...eventProgress, cancel_failed: true })
    await waitFor(() => expect(cancel.disabled).toBe(true))
  })
})
