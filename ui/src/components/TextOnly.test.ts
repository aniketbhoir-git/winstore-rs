// SPEC 6.6 "Trust boundary", rule 2: catalog text is shown as text, never as HTML.
import { render, screen } from '@testing-library/svelte'
import { describe, expect, it } from 'vitest'
import type { PackageSummary } from '../lib/types'
import { eventProgress, installBridge, mockBackend } from '../test/ipc'
import AppCard from './AppCard.svelte'

const markup = '<img src=x onerror="alert(1)"><b>bold</b><script>alert(2)</script>'

describe('catalog text', () => {
  it('(a, b, c) catalog text with markup appears as text', () => {
    installBridge()
    mockBackend({})
    const pkg: PackageSummary = {
      id: eventProgress.package_id,
      name: `Name ${markup}`,
      publisher: `Pub ${markup}`,
      latest_version: '1.0',
      description: `Desc ${markup}`,
      icon_url: null,
      rating: null,
      rating_count: null,
      source: 'winget_community',
      is_paid: false,
      packaging: 'msi',
    }
    const { container } = render(AppCard, { pkg })
    expect(screen.getByText(`Desc ${markup}`)).toBeTruthy()
    expect(container.textContent).toContain(`Name ${markup}`)
    expect(container.querySelector('img, b, script')).toBeNull()
  })

  it('(a, b, c) no component renders raw HTML', () => {
    const files = import.meta.glob('../**/*.svelte', {
      query: '?raw',
      import: 'default',
      eager: true,
    })
    expect(Object.keys(files).length).toBeGreaterThan(5)
    for (const [path, src] of Object.entries(files)) {
      expect(src as string, path).not.toMatch(/\{@html\b/)
    }
  })
})
