// Live state of installs/uninstalls/updates, keyed by package id, fed by backend events.
import { api, events, toAppError } from './api'
import type {
  ActiveOperation,
  AppError,
  InstallOptions,
  OperationKind,
  OperationStage,
  PackageId,
} from './types'

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

/** One line of the live log tail. */
export interface LiveLine {
  ts: number
  operationId: string
  stage: OperationStage
  message: string
}

const TERMINAL: OperationStage[] = ['done', 'failed', 'cancelled']
/** Live lines kept per package. */
const LIVE_MAX = 200

class Operations {
  byPackage = $state<Record<PackageId, OpState>>({})
  /** Progress messages of recent operations, oldest first (log tail pane). */
  lines = $state<Record<PackageId, LiveLine[]>>({})
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
        this.addLine(e.package_id, {
          ts: e.timestamp,
          operationId: e.operation_id,
          stage: e.stage,
          message: e.message,
        })
      }),
      events.onOperationFinished((e) => {
        const cur = this.byPackage[e.package_id]
        if (!cur) return
        this.byPackage[e.package_id] = {
          ...cur,
          operationId: e.operation_id,
          finished: true,
          stage: e.ok ? 'done' : e.error?.code === 'cancelled' ? 'cancelled' : 'failed',
          fraction: e.ok ? 1 : cur.fraction,
          error: e.ok || e.error?.code === 'cancelled' ? null : e.error,
        }
      }),
    ])
  }

  private addLine(id: PackageId, line: LiveLine) {
    const list = this.lines[id] ?? []
    const last = list[list.length - 1]
    // Spinner and byte counters repeat; keep one line per stage/message change.
    if (last && last.operationId === line.operationId && last.message === line.message) return
    this.lines[id] = [...list, line].slice(-LIVE_MAX)
  }

  get(id: PackageId): OpState | undefined {
    return this.byPackage[id]
  }

  /** True while an operation for `id` is starting or running. */
  isRunning(id: PackageId): boolean {
    const op = this.byPackage[id]
    return !!op && !op.finished
  }

  /** Attaches to an operation reported by `get_install_status` (for example after a reload). */
  adopt(id: PackageId, active: ActiveOperation) {
    const cur = this.byPackage[id]
    if (cur && cur.operationId === active.operation_id) return
    this.byPackage[id] = {
      operationId: active.operation_id,
      operation: active.operation,
      stage: 'queued',
      fraction: 0,
      message: 'Running',
      finished: false,
      error: null,
    }
  }

  install(id: PackageId, options?: InstallOptions) {
    return this.start(id, 'install', () => api.startInstall(id, options))
  }

  uninstall(id: PackageId) {
    return this.start(id, 'remove', () => api.startUninstall(id))
  }

  private async start(
    id: PackageId,
    operation: OperationKind,
    begin: () => Promise<{ operation_id: string }>,
  ) {
    // Optimistic row so the buttons react before the first event arrives.
    this.byPackage[id] = {
      operationId: '',
      operation,
      stage: 'queued',
      fraction: 0,
      message: 'Starting',
      finished: false,
      error: null,
    }
    try {
      const { operation_id } = await begin()
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
