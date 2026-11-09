// Live state of installs/updates, keyed by package id, fed by backend events.
import { api, events, toAppError } from './api'
import type { AppError, OperationKind, OperationStage, PackageId } from './types'

export interface OpState {
  operationId: string
  operation: OperationKind
  stage: OperationStage
  fraction: number
  message: string
  /** Set once the operation ended. */
  finished: boolean
  error: AppError | null
}

const TERMINAL: OperationStage[] = ['done', 'failed', 'cancelled']

class Operations {
  byPackage = $state<Record<PackageId, OpState>>({})
  private started = false

  /** Subscribes once to progress / finished events; safe to call repeatedly. */
  async init() {
    if (this.started) return
    this.started = true
    await Promise.all([
      events.onProgress((e) => {
        const prev = this.byPackage[e.package_id]
        this.byPackage[e.package_id] = {
          operationId: e.operation_id,
          operation: e.operation,
          stage: e.stage,
          fraction: e.fraction,
          message: e.message,
          finished: TERMINAL.includes(e.stage),
          error: prev && prev.operationId === e.operation_id ? prev.error : null,
        }
      }),
      events.onOperationFinished((e) => {
        const cur = this.byPackage[e.package_id]
        if (!cur) return
        this.byPackage[e.package_id] = {
          ...cur,
          finished: true,
          stage: e.ok ? 'done' : cur.stage === 'cancelled' ? 'cancelled' : 'failed',
          fraction: e.ok ? 1 : cur.fraction,
          error: e.error,
        }
      }),
    ])
  }

  get(id: PackageId): OpState | undefined {
    return this.byPackage[id]
  }

  async install(id: PackageId) {
    // Optimistic row so the button reacts before the first event arrives.
    this.byPackage[id] = {
      operationId: '',
      operation: 'install',
      stage: 'queued',
      fraction: 0,
      message: 'Starting',
      finished: false,
      error: null,
    }
    try {
      const { operation_id } = await api.startInstall(id)
      const cur = this.byPackage[id]
      if (cur && !cur.operationId) cur.operationId = operation_id
    } catch (e) {
      this.byPackage[id] = {
        ...this.byPackage[id],
        stage: 'failed',
        finished: true,
        error: toAppError(e),
      }
    }
  }

  async cancel(id: PackageId) {
    const op = this.byPackage[id]
    if (!op?.operationId) return
    try {
      await api.cancelInstall(op.operationId)
    } catch {
      // The finished event reports the real outcome.
    }
  }
}

export const operations = new Operations()
