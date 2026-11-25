// SPEC §6.6 case (b), UI tests tagged (a, b): the consent dialog, Accept, Cancel, text only.
import { emit } from '@tauri-apps/api/event'
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { beforeAll, beforeEach, describe, expect, it } from 'vitest'
import type { AppError, PackageSummary } from '../lib/types'
import { operations } from '../lib/operations.svelte'
import { errorAgreementsRequired, eventProgress, installBridge, mockBackend } from '../test/ipc'
import DetailActions from './DetailActions.svelte'
import InstallButton from './InstallButton.svelte'

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

const agreement = (over: Partial<AppError> = {}): AppError => ({
  ...(errorAgreementsRequired as AppError),
  ...over,
})

/** Clicks Install, lets the operation end with `error`, and returns the call log. */
async function installEndingWith(error: AppError, replies: Record<string, () => unknown> = {}) {
  const calls = mockBackend({
    start_install: () => ({ operation_id: 'op-1' }),
    open_agreement_link: () => undefined,
    ...replies,
  })
  render(InstallButton, { pkg })
  await fireEvent.click(screen.getByRole('button', { name: 'Install' }))
  await waitFor(() => expect(operations.get(pkg.id)?.operationId).toBe('op-1'))
  await emit('store:operation-finished', {
    operation_id: 'op-1',
    package_id: pkg.id,
    ok: false,
    error,
  })
  return calls
}

describe('consent dialog', () => {
  beforeAll(async () => {
    installBridge()
    await operations.init()
  })
  beforeEach(() => {
    operations.byPackage = {}
    operations.lines = {}
    operations.declineAgreement(pkg.id)
  })

  it('(a, b) agreements_required with a link opens the dialog and shows no error', async () => {
    await installEndingWith(agreement())
    const dialog = await screen.findByRole('dialog', { name: 'Agreement needed' })
    expect(dialog.getAttribute('aria-modal')).toBe('true')
    expect(dialog.textContent).toContain('Foo Bar 1.2.3')
    expect(dialog.textContent).toContain('winget')
    expect(dialog.textContent).toContain('Terms of use')
    expect(dialog.textContent).toContain('https://example.org/terms')
    expect(screen.queryByRole('alert')).toBeNull()
    // Focus starts on the dialog, so a stray Enter cannot accept.
    await waitFor(() => expect(document.activeElement).toBe(dialog))
  })

  it('(a, b) with agreement text and no link, the dialog opens too', async () => {
    await installEndingWith(
      agreement({
        agreement_url: null,
        agreements: [{ label: 'EULA', text: 'You may use this app.', url: null }],
        agreement_text: 'You may use this app.',
      }),
    )
    const dialog = await screen.findByRole('dialog')
    expect(dialog.textContent).toContain('EULA')
    expect(dialog.textContent).toContain('You may use this app.')
    expect(screen.queryByRole('button', { name: /Open in browser/ })).toBeNull()
  })

  it('(a, b) Accept starts a new operation with a consent bound to this package and version', async () => {
    const calls = await installEndingWith(agreement({ version: '1.2.3' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'Accept' }))
    await waitFor(() => expect(calls.filter((c) => c.cmd === 'start_install')).toHaveLength(2))
    expect(calls.filter((c) => c.cmd === 'start_install')[1]).toEqual({
      cmd: 'start_install',
      args: {
        packageId: pkg.id,
        options: {
          version: '1.2.3',
          consent: { kinds: ['package'], tokens: [errorAgreementsRequired.consent_token] },
        },
      },
    })
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('(a, b) the retry of a second kind carries both kinds with their tokens', async () => {
    const calls = await installEndingWith(
      agreement({ source_agreements: true, package_agreements: false, consent_token: 'tok-source' }),
    )
    await fireEvent.click(await screen.findByRole('button', { name: 'Accept' }))
    await waitFor(() => expect(calls.filter((c) => c.cmd === 'start_install')).toHaveLength(2))
    await waitFor(() => expect(operations.get(pkg.id)?.operationId).toBe('op-1'))
    await emit('store:operation-finished', {
      operation_id: 'op-1',
      package_id: pkg.id,
      ok: false,
      error: agreement({ consent_token: 'tok-package' }),
    })
    await fireEvent.click(await screen.findByRole('button', { name: 'Accept' }))
    await waitFor(() => expect(calls.filter((c) => c.cmd === 'start_install')).toHaveLength(3))
    const third = calls.filter((c) => c.cmd === 'start_install')[2].args as {
      options: { consent: unknown }
    }
    expect(third.options.consent).toEqual({
      kinds: ['source', 'package'],
      tokens: ['tok-source', 'tok-package'],
    })
  })

  it('(a, b) Cancel makes no call and ends the attempt', async () => {
    const calls = await installEndingWith(agreement())
    await fireEvent.click(await screen.findByRole('button', { name: 'Cancel' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
    expect(calls.map((c) => c.cmd)).toEqual(['start_install'])
    expect(screen.queryByRole('alert')).toBeNull()
    // A new click starts a new attempt with no consent.
    await fireEvent.click(screen.getByRole('button', { name: 'Install' }))
    await waitFor(() => expect(calls.filter((c) => c.cmd === 'start_install')).toHaveLength(2))
    expect(calls[1].args).toEqual({ packageId: pkg.id, options: undefined })
  })

  it('(a, b) Escape is Cancel; Tab stays inside the dialog', async () => {
    const calls = await installEndingWith(agreement())
    const dialog = await screen.findByRole('dialog')
    const accept = screen.getByRole('button', { name: 'Accept' })
    accept.focus()
    await fireEvent.keyDown(accept, { key: 'Tab' })
    expect(document.activeElement).not.toBe(document.body)
    expect(dialog.contains(document.activeElement)).toBe(true)
    await fireEvent.keyDown(dialog, { key: 'Escape' })
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
    expect(calls.map((c) => c.cmd)).toEqual(['start_install'])
  })

  it('(a, b) the link opens only through open_agreement_link, for this package', async () => {
    const calls = await installEndingWith(agreement())
    await fireEvent.click(await screen.findByRole('button', { name: /Open in browser/ }))
    await waitFor(() => expect(calls.some((c) => c.cmd === 'open_agreement_link')).toBe(true))
    expect(calls.find((c) => c.cmd === 'open_agreement_link')?.args).toEqual({
      packageId: pkg.id,
      url: 'https://example.org/terms',
    })
    expect(document.querySelector('a')).toBeNull()
  })

  it('(a, b) the agreement shows as text, not HTML', async () => {
    const markup = '<img src=x onerror="alert(1)"><b>bold</b><script>alert(2)</script>'
    await installEndingWith(
      agreement({
        agreements: [{ label: `L ${markup}`, text: `T ${markup}`, url: 'https://example.org/a?b=<b>' }],
        agreement_url: 'https://example.org/a?b=<b>',
      }),
    )
    const dialog = await screen.findByRole('dialog')
    expect(dialog.textContent).toContain(`L ${markup}`)
    expect(dialog.textContent).toContain(`T ${markup}`)
    expect(dialog.querySelector('img, b, script, a')).toBeNull()
  })

  it('(a, b) an asked-again kind after Accept is an error, not a second dialog', async () => {
    const calls = await installEndingWith(agreement())
    await fireEvent.click(await screen.findByRole('button', { name: 'Accept' }))
    await waitFor(() => expect(calls.filter((c) => c.cmd === 'start_install')).toHaveLength(2))
    await waitFor(() => expect(operations.get(pkg.id)?.operationId).toBe('op-1'))
    await emit('store:operation-finished', {
      operation_id: 'op-1',
      package_id: pkg.id,
      ok: false,
      error: agreement(),
    })
    await screen.findByRole('alert')
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('(a, b) an uninstall asks for the source kind only; a package kind is an error', async () => {
    const calls = mockBackend({ start_uninstall: () => ({ operation_id: 'op-u' }) })
    render(DetailActions, {
      pkg,
      versions: [],
      status: {
        package_id: pkg.id,
        state: 'installed',
        installed_version: '1.0',
        target_version: null,
        pinned: false,
        last_updated: null,
        active: null,
      },
    })
    await fireEvent.click(screen.getByRole('button', { name: 'Uninstall' }))
    await waitFor(() => expect(operations.get(pkg.id)?.operationId).toBe('op-u'))
    await emit('store:operation-finished', {
      operation_id: 'op-u',
      package_id: pkg.id,
      ok: false,
      error: agreement({
        source_agreements: true,
        package_agreements: false,
        consent_token: 'tok-source',
        version: null,
      }),
    })
    await fireEvent.click(await screen.findByRole('button', { name: 'Accept' }))
    await waitFor(() => expect(calls.filter((c) => c.cmd === 'start_uninstall')).toHaveLength(2))
    expect(calls[1]).toEqual({
      cmd: 'start_uninstall',
      args: {
        packageId: pkg.id,
        options: { consent: { kinds: ['source'], tokens: ['tok-source'] } },
      },
    })
    await waitFor(() => expect(operations.get(pkg.id)?.operationId).toBe('op-u'))
    await emit('store:operation-finished', {
      operation_id: 'op-u',
      package_id: pkg.id,
      ok: false,
      error: agreement({ version: null }),
    })
    await screen.findByRole('alert')
    expect(screen.queryByRole('dialog')).toBeNull()
  })
})
