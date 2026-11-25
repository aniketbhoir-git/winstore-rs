// Hardening design 4, import step 6: the review list. A tick per package, no "select all",
// file text as text, nothing starts an install.
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { emit } from '@tauri-apps/api/event'
import { beforeAll, describe, expect, it } from 'vitest'
import { requestImport } from '../lib/importRequest.svelte'
import { installBridge, mockBackend } from '../test/ipc'
import ImportPanel from './ImportPanel.svelte'

beforeAll(() => installBridge())

const review = {
  file_name: 'mine.json',
  installed_known: true,
  rows: [
    { package_id: 'VideoLAN.VLC', version: '3.0.21', source: 'winget', status: 'ready' },
    { package_id: 'Git.Git', version: null, source: 'winget', status: 'installed' },
    { package_id: '--bad', version: null, source: 'winget', status: 'not_valid' },
    { package_id: '9NBL', version: null, source: 'msstore', status: 'source_not_supported' },
    { package_id: 'Mozilla.Firefox', version: null, source: 'winget', status: 'ready' },
  ],
}

describe('ImportPanel', () => {
  it('reviews a dropped file: per-package ticks, only ready rows can be ticked', async () => {
    const calls = mockBackend({ read_import_file: () => review })
    render(ImportPanel)
    await waitFor(() => expect(screen.getByText(/Drop a winget export file/)).toBeTruthy())
    await emit('store:import-dropped', { path: 'C:\\x\\mine.json', file_name: 'mine.json' })
    await waitFor(() => expect(screen.getByText('mine.json')).toBeTruthy())
    expect(calls.find((c) => c.cmd === 'read_import_file')?.args).toEqual({
      path: 'C:\\x\\mine.json',
    })
    const boxes = screen.getAllByRole('checkbox') as HTMLInputElement[]
    expect(boxes.map((b) => b.disabled)).toEqual([false, true, true, true, false])
    expect(boxes.some((b) => b.checked)).toBe(false)
    expect(screen.getByText(/0 selected/)).toBeTruthy()
    expect(screen.queryByText(/select all/i)).toBeNull()
    await fireEvent.click(boxes[0])
    await fireEvent.click(boxes[4])
    expect(screen.getByText(/2 selected/)).toBeTruthy()
    expect(screen.getByText('Installed')).toBeTruthy()
    expect(screen.getByText('Not valid')).toBeTruthy()
    expect(screen.getByText('Source not supported')).toBeTruthy()
    expect(calls.map((c) => c.cmd)).not.toContain('start_install')
  })

  it('reviews a file from the exports folder on request', async () => {
    const calls = mockBackend({ read_import_file: () => review })
    render(ImportPanel)
    requestImport('C:\\Docs\\Store\\exports\\store-export-2026-11-25.json')
    await waitFor(() => expect(screen.getByText('mine.json')).toBeTruthy())
    expect(calls.find((c) => c.cmd === 'read_import_file')?.args).toEqual({
      path: 'C:\\Docs\\Store\\exports\\store-export-2026-11-25.json',
    })
  })

  it('shows markup from the file as text and an error as an alert', async () => {
    const markup = '<img src=x onerror="alert(1)">'
    mockBackend({
      read_import_file: () => ({
        ...review,
        rows: [{ package_id: markup, version: markup, source: 'winget', status: 'not_valid' }],
      }),
    })
    const { container } = render(ImportPanel)
    requestImport('a.json')
    await waitFor(() => expect(container.textContent).toContain(markup))
    expect(container.querySelector('img')).toBeNull()

    mockBackend({
      read_import_file: () => {
        throw { code: 'invalid_config', message: 'not allowed', detail: null, retryable: false }
      },
    })
    requestImport('b.json')
    await waitFor(() => expect(screen.getByRole('alert').textContent).toMatch(/not allowed/))
  })

  it('keys ticks by id and source', async () => {
    mockBackend({
      read_import_file: () => ({
        ...review,
        rows: [
          { package_id: 'Same.Id', version: null, source: 'winget', status: 'ready' },
          { package_id: 'Same.Id', version: null, source: 'winget-other', status: 'ready' },
        ],
      }),
    })
    render(ImportPanel)
    requestImport('c.json')
    await waitFor(() => expect(screen.getAllByRole('checkbox').length).toBe(2))
    const boxes = screen.getAllByRole('checkbox') as HTMLInputElement[]
    await fireEvent.click(boxes[0])
    expect(boxes.map((b) => b.checked)).toEqual([true, false])
    expect(screen.getByText(/1 selected/)).toBeTruthy()
  })
})
