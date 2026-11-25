// SPEC §6.10: the update-scan setting shows the saved mode and saves a change.
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { beforeAll, describe, expect, it } from 'vitest'
import { installBridge, mockBackend } from '../test/ipc'
import UpdateScanSetting from './UpdateScanSetting.svelte'

beforeAll(() => installBridge())

describe('UpdateScanSetting', () => {
  it('shows the saved mode and saves a change', async () => {
    const calls = mockBackend({ get_update_scan: () => 'daily', set_update_scan: () => undefined })
    render(UpdateScanSetting)
    const select = screen.getByRole('combobox') as HTMLSelectElement
    await waitFor(() => expect(select.value).toBe('daily'))
    await fireEvent.change(select, { target: { value: 'weekly' } })
    await waitFor(() =>
      expect(calls.find((c) => c.cmd === 'set_update_scan')?.args).toEqual({ mode: 'weekly' }),
    )
  })

  it('goes back to the old mode and shows the error when saving fails', async () => {
    mockBackend({
      get_update_scan: () => 'off',
      set_update_scan: () => {
        throw { code: 'invalid_config', message: 'not saved', detail: null, retryable: false }
      },
    })
    render(UpdateScanSetting)
    const select = screen.getByRole('combobox') as HTMLSelectElement
    await fireEvent.change(select, { target: { value: 'daily' } })
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('not saved'))
    expect(select.value).toBe('off')
  })
})
