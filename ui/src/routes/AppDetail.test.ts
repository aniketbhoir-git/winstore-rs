// L2 tests for the detail page states (docs/designs/catalog-winget-cli.md, steps 5 and 6.x).
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { beforeAll, describe, expect, it } from 'vitest'
import { READ_ONLY_AGREEMENT_TEXT } from '../lib/errors'
import { operations } from '../lib/operations.svelte'
import type { PackageDetail, PackageSummary } from '../lib/types'
import { errorAgreementsRequired, eventProgress, installBridge, mockBackend } from '../test/ipc'
import AppCard from '../components/AppCard.svelte'
import Search from './Search.svelte'
import AppDetail from './AppDetail.svelte'

const id = eventProgress.package_id

const summary: PackageSummary = {
  id,
  name: 'Foo Bar',
  publisher: 'Example Publisher',
  latest_version: '1.2.3',
  description: 'A package',
  icon_url: null,
  rating: null,
  rating_count: null,
  source: 'winget_community',
  is_paid: false,
  packaging: 'unknown',
}

/** What a non-English Windows gives: the title line and the id parse, no key does. */
const bare: PackageDetail = {
  summary: { ...summary, publisher: '', description: '' },
  full_description: '',
  screenshots: [],
  website: null,
  license: null,
  release_notes: null,
  installer: { architecture: null, install_type: 'unknown', size_bytes: null, sha256: null },
  versions: {},
}

const status = {
  package_id: id,
  state: 'not_installed',
  installed_version: null,
  target_version: null,
  pinned: false,
  last_updated: null,
  active: null,
}

const backend = (getDetail: () => unknown) =>
  mockBackend({
    get_app_detail: getDetail,
    get_install_status: () => status,
    get_operation_log: () => [],
  })

describe('AppDetail states', () => {
  beforeAll(async () => {
    installBridge()
    await operations.init()
  })

  it('shows a spinner and the slow-first-show text while the detail loads', async () => {
    let release: (d: PackageDetail) => void = () => {}
    backend(() => new Promise<PackageDetail>((r) => (release = r)))
    render(AppDetail, { id, onback: () => {} })
    const loading = await screen.findByRole('status')
    expect(loading.textContent).toContain(
      'Loading details from winget. The first time can take up to a minute.',
    )
    release({ ...bare, summary })
    await waitFor(() => expect(screen.queryByRole('status')).toBeNull())
    expect(screen.getByRole('heading', { name: 'Foo Bar' })).toBeTruthy()
  })

  it('shows the 60 s limit message with a retry after source_unavailable', async () => {
    let n = 0
    backend(() => {
      if (n++ === 0) {
        throw {
          code: 'source_unavailable',
          message: 'winget did not answer in 60 s',
          detail: null,
          retryable: true,
        }
      }
      return { ...bare, summary }
    })
    render(AppDetail, { id, onback: () => {} })
    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain('winget did not answer in 60 s')
    await fireEvent.click(screen.getByRole('button', { name: 'Try again' }))
    expect(await screen.findByRole('heading', { name: 'Foo Bar' })).toBeTruthy()
  })

  it('shows the read-only agreement text for agreements_required (fixture)', async () => {
    backend(() => {
      throw errorAgreementsRequired
    })
    render(AppDetail, { id, onback: () => {} })
    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain(READ_ONLY_AGREEMENT_TEXT)
    expect(alert.textContent).not.toContain(errorAgreementsRequired.message)
    expect(alert.textContent).toContain('winget search')
  })

  it('hides an empty publisher on the page and in the information list', async () => {
    backend(() => bare)
    const { container } = render(AppDetail, { id, onback: () => {} })
    await screen.findByRole('heading', { name: 'Foo Bar' })
    expect(container.querySelector('.publisher')).toBeNull()
    expect(screen.queryByText('Publisher')).toBeNull()
  })

  it('README limit: name, id and version only, with no empty sections', async () => {
    backend(() => bare)
    const { container } = render(AppDetail, { id, onback: () => {} })
    await screen.findByRole('heading', { name: 'Foo Bar' })
    expect(screen.getByText('v1.2.3')).toBeTruthy()
    expect(screen.getAllByText(id).length).toBeGreaterThan(0)
    expect(screen.queryByRole('heading', { name: 'Description' })).toBeNull()
    expect(screen.queryByText('Installer')).toBeNull()
    expect(container.querySelector('dd:empty')).toBeNull()
  })

  it('keeps the publisher when the detail has one', async () => {
    backend(() => ({ ...bare, summary }))
    render(AppDetail, { id, onback: () => {} })
    await screen.findByRole('heading', { name: 'Foo Bar' })
    expect(screen.getAllByText('Example Publisher').length).toBeGreaterThan(0)
  })
})

describe('search card and search errors', () => {
  it('hides an empty publisher on the card', () => {
    installBridge()
    mockBackend({})
    const { container } = render(AppCard, { pkg: { ...summary, publisher: '' } })
    expect(container.querySelector('.publisher')).toBeNull()
    expect(screen.getByText('Foo Bar')).toBeTruthy()
  })

  it('shows the read-only agreement text when a search fails with agreements_required', async () => {
    installBridge()
    mockBackend({
      search_sources: () => {
        throw errorAgreementsRequired
      },
    })
    render(Search, {})
    await fireEvent.input(screen.getByRole('searchbox'), { target: { value: 'vlc' } })
    await fireEvent.submit(screen.getByRole('search'))
    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain(READ_ONLY_AGREEMENT_TEXT)
  })
})
