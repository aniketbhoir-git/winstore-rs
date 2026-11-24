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
  /** No download bar was seen: `fraction` is a stage estimate, not a measured value. */
  indeterminate: boolean
  message: string
  /** Set once the operation ended. */
  finished: boolean
  error: AppError | null
  /** A progress event reported `cancel_failed`: the installer continues, Cancel is disabled. */
  cancelFailed: boolean
}

/** One line of the live log tail. */
export interface LiveLine {
  ts: number
  operationId: string
  stage: OperationStage
  message: string
}

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
          indeterminate: e.indeterminate === true,
          message: e.message,
          // Only `store:operation-finished` ends an operation: it comes after the ledger is
          // settled, so a terminal progress event must not reveal stale buttons.
          finished: false,
          error: prev && prev.operationId === e.operation_id ? prev.error : null,
          // Read the flag, never the message text. Once set it stays for this operation.
          cancelFailed:
            e.cancel_failed === true ||
            (!!prev && prev.operationId === e.operation_id && prev.cancelFailed),
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
        // A late event of an earlier operation must not finish the current one. An empty id
        // means the invoke reply has not arrived yet; the event then names the operation.
        if (cur.operationId && cur.operationId !== e.operation_id) return
        this.byPackage[e.package_id] = {
          ...cur,
          operationId: e.operation_id,
          finished: true,
          stage: e.ok ? 'done' : e.error?.code === 'cancelled' ? 'cancelled' : 'failed',
          fraction: e.ok ? 1 : cur.fraction,
          indeterminate: false,
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
      indeterminate: true,
      message: 'Running',
      finished: false,
      error: null,
      cancelFailed: false,
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
      indeterminate: true,
      message: 'Starting',
      finished: false,
      error: null,
      cancelFailed: false,
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
