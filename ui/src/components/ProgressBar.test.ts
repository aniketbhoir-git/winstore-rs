// L2: determinate and indeterminate states of ProgressBar (SPEC §4.2, `indeterminate`).
import { render, screen } from '@testing-library/svelte'
import { describe, expect, it } from 'vitest'
import ProgressBar from './ProgressBar.svelte'

describe('ProgressBar', () => {
  it('shows a measured value with aria-valuenow when determinate', () => {
    render(ProgressBar, { fraction: 0.5, label: 'Downloading' })
    const bar = screen.getByRole('progressbar', { name: 'Downloading' })
    expect(bar.getAttribute('aria-valuenow')).toBe('50')
    expect(bar.getAttribute('aria-valuemin')).toBe('0')
    expect(bar.getAttribute('aria-valuemax')).toBe('100')
    expect(bar.getAttribute('aria-busy')).toBeNull()
    expect(bar.classList.contains('indeterminate')).toBe(false)
  })

  it('is busy with no value when indeterminate, and still shows the stage text', () => {
    render(ProgressBar, {
      fraction: 0.7,
      indeterminate: true,
      label: 'installing: Starting package install...',
    })
    const bar = screen.getByRole('progressbar', { name: /installing/ })
    expect(bar.getAttribute('aria-valuenow')).toBeNull()
    expect(bar.getAttribute('aria-valuemin')).toBeNull()
    expect(bar.getAttribute('aria-valuemax')).toBeNull()
    expect(bar.getAttribute('aria-busy')).toBe('true')
    expect(bar.classList.contains('indeterminate')).toBe(true)
    // The estimated fraction is never drawn as a width.
    expect(bar.querySelector<HTMLElement>('.fill')?.style.width).toBe('')
    expect(screen.getByText('installing: Starting package install...')).toBeTruthy()
  })
})
