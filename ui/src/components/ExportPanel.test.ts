// Hardening design 4, export: the button, the result as text, Open folder, the error text.
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { beforeAll, describe, expect, it } from 'vitest'
import { installBridge, mockBackend } from '../test/ipc'
import ExportPanel from './ExportPanel.svelte'

beforeAll(() => installBridge())

describe('ExportPanel', () => {
  it('exports and shows the path as text, then opens the folder', async () => {
    const calls = mockBackend({
      export_packages: () => ({
        path: 'C:\\Users\\a\\Documents\\Store\\exports\\store-export-2026-11-25.json',
        file_name: 'store-export-2026-11-25.json',
        package_count: 3,
      }),
      open_exports_folder: () => undefined,
    })
    render(ExportPanel)
    await fireEvent.click(screen.getByRole('button', { name: 'Export installed apps' }))
    await waitFor(() => expect(screen.getByText(/store-export-2026-11-25\.json/)).toBeTruthy())
    expect(screen.getByText(/Saved 3 apps/)).toBeTruthy()
    await fireEvent.click(screen.getByRole('button', { name: 'Open folder' }))
    await waitFor(() => expect(calls.map((c) => c.cmd)).toContain('open_exports_folder'))
    // No argument leaves the webview: the core owns the path.
    expect(calls.find((c) => c.cmd === 'export_packages')?.args).toEqual({})
  })

  it('shows the terminal hint for agreements_required', async () => {
    mockBackend({
      export_packages: () => {
        throw { code: 'agreements_required', message: 'x', detail: null, retryable: false }
      },
    })
    render(ExportPanel)
    await fireEvent.click(screen.getByRole('button', { name: 'Export installed apps' }))
    await waitFor(() =>
      expect(screen.getByRole('alert').textContent).toMatch(/terms of the winget source/),
    )
  })

  it('explains the 500 app limit', async () => {
    mockBackend({
      export_packages: () => {
        throw {
          code: 'winget_failed',
          message: 'winget failed',
          detail: 'export the file holds more than 500 packages',
          retryable: true,
        }
      },
    })
    render(ExportPanel)
    await fireEvent.click(screen.getByRole('button', { name: 'Export installed apps' }))
    await waitFor(() =>
      expect(screen.getByRole('alert').textContent).toMatch(/more than 500 apps.*nothing was saved/),
    )
  })

  it('shows the message of another error', async () => {
    mockBackend({
      export_packages: () => {
        throw { code: 'winget_failed', message: 'winget failed', detail: null, retryable: true }
      },
    })
    render(ExportPanel)
    await fireEvent.click(screen.getByRole('button', { name: 'Export installed apps' }))
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('winget failed'))
  })
})
