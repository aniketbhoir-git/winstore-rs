// L2: the scope choice reaches start_install; the banner follows app_status.
import { emit } from '@tauri-apps/api/event'
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { beforeAll, describe, expect, it } from 'vitest'
import type { PackageSummary } from '../lib/types'
import { errorText } from '../lib/errors'
import { operations } from '../lib/operations.svelte'
import { eventProgress, installBridge, mockBackend } from '../test/ipc'
import AdminBanner from './AdminBanner.svelte'
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

const BANNER = 'Store runs as administrator. Installers will not ask before they change the system.'

describe('install scope choice', () => {
  beforeAll(async () => {
    installBridge()
    await operations.init()
  })

  async function installWith(choice: string | null) {
    operations.byPackage = {}
    const calls = mockBackend({ start_install: () => ({ operation_id: 'op-1' }) })
    render(DetailActions, { pkg, versions: [], status: null })
    if (choice !== null) {
      await fireEvent.change(screen.getByLabelText('Install scope'), { target: { value: choice } })
    }
    await fireEvent.click(screen.getByRole('button', { name: 'Install' }))
    await waitFor(() => expect(calls).toHaveLength(1))
    return calls[0]
  }

  it('Default sends no scope', async () => {
    const call = await installWith(null)
    expect(call).toEqual({ cmd: 'start_install', args: { packageId: pkg.id, options: undefined } })
  })

  it.each(['user', 'machine'] as const)('%s sends the value', async (scope) => {
    const call = await installWith(scope)
    expect(call.args).toEqual({ packageId: pkg.id, options: { scope } })
  })
})

describe('administrator banner', () => {
  it('shows when the app is elevated', async () => {
    mockBackend({ app_status: () => ({ elevated: true }) })
    render(AdminBanner)
    expect(await screen.findByText(BANNER)).toBeTruthy()
  })

  it('stays away when the app is not elevated', async () => {
    const calls = mockBackend({ app_status: () => ({ elevated: false }) })
    render(AdminBanner)
    await waitFor(() => expect(calls).toHaveLength(1))
    expect(screen.queryByText(BANNER)).toBeNull()
  })
})

describe('hardening texts', () => {
  it('elevation_required shows the text of design section 1 rule 6', () => {
    expect(
      errorText({ code: 'elevation_required', message: 'raw', detail: null, retryable: false }),
    ).toBe(
      "This app needs administrator rights to install. Windows did not give them. Try again and accept the Windows prompt, or choose 'Only for me' if the app supports it.",
    )
  })

  it('cancel_failed shows the text of the elevated installer case', async () => {
    operations.byPackage = {}
    const calls = mockBackend({ start_install: () => ({ operation_id: 'op-1' }) })
    render(DetailActions, { pkg, versions: [], status: null })
    await fireEvent.click(screen.getByRole('button', { name: 'Install' }))
    await waitFor(() => expect(calls).toHaveLength(1))
    await emit('store:progress', eventProgress)
    const cancel = (await screen.findByRole('button', { name: 'Cancel' })) as HTMLButtonElement
    await waitFor(() => expect(cancel.disabled).toBe(false))
    await emit('store:progress', { ...eventProgress, cancel_failed: true })
    expect(
      await screen.findByText('The installer runs with administrator rights. Store cannot stop it.'),
    ).toBeTruthy()
  })
})
