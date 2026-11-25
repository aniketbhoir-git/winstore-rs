// Live state of installs/uninstalls/updates, keyed by package id, fed by backend events.
import { api, events, toAppError } from './api'
import { hasAgreementDialog } from './errors'
import type {
  ActiveOperation,
  AgreementKind,
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
  /**
   * An `agreements_required` error that the consent dialog must show (SPEC §6.6 step 3). Set
   * instead of `error`; cleared by Accept (the retry) or Decline.
   */
  agreement: AppError | null
}

/**
 * One user action and what the user accepted in it (SPEC §6.6). It lives in memory only, and
 * a new click on Install or Uninstall starts a new one with nothing accepted.
 */
interface Attempt {
  operation: 'install' | 'remove'
  options?: InstallOptions
  /** `consent_token` of each kind that the user accepted in this attempt. */
  accepted: Partial<Record<AgreementKind, string>>
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
  private attempts = new Map<PackageId, Attempt>()

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
          agreement: null,
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
        // SPEC §6.6 step 3: winget needs an agreement and there is something to show. Open the
        // dialog and show no error. When the user already accepted this kind in this attempt,
        // winget asking again is an error, not a second dialog.
        const kind: AgreementKind | null =
          !e.ok && e.error && hasAgreementDialog(e.error)
            ? e.error.source_agreements
              ? 'source'
              : 'package'
            : null
        const at = this.attempts.get(e.package_id)
        if (kind && e.error && at && !at.accepted[kind] && !(at.operation === 'remove' && kind === 'package')) {
          this.byPackage[e.package_id] = {
            ...cur,
            operationId: e.operation_id,
            finished: true,
            stage: 'failed',
            indeterminate: false,
            error: null,
            agreement: e.error,
          }
          return
        }
        this.byPackage[e.package_id] = {
          ...cur,
          operationId: e.operation_id,
          finished: true,
          stage: e.ok ? 'done' : e.error?.code === 'cancelled' ? 'cancelled' : 'failed',
          fraction: e.ok ? 1 : cur.fraction,
          indeterminate: false,
          error: e.ok || e.error?.code === 'cancelled' ? null : e.error,
          agreement: null,
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
    if (cur && cur.operationId === active.operation_id) {
      if (active.cancel_failed === true) cur.cancelFailed = true
      return
    }
    this.byPackage[id] = {
      operationId: active.operation_id,
      operation: active.operation,
      stage: 'queued',
      fraction: 0,
      indeterminate: true,
      message: 'Running',
      finished: false,
      error: null,
      cancelFailed: active.cancel_failed === true,
      agreement: null,
    }
  }

  install(id: PackageId, options?: InstallOptions) {
    this.attempts.set(id, { operation: 'install', options, accepted: {} })
    return this.start(id, 'install', () => api.startInstall(id, options))
  }

  uninstall(id: PackageId) {
    this.attempts.set(id, { operation: 'remove', accepted: {} })
    return this.start(id, 'remove', () => api.startUninstall(id))
  }

  /**
   * "Accept" in the consent dialog (SPEC §6.6 step 4): starts a new operation of the same
   * attempt with every kind accepted so far, each with its token, and with the version of the
   * error. The consent is built here from what the user accepted just now; nothing is stored.
   */
  acceptAgreement(id: PackageId) {
    const cur = this.byPackage[id]
    const err = cur?.agreement
    const at = this.attempts.get(id)
    if (!cur || !err || !at || !err.consent_token) return
    at.accepted[err.source_agreements ? 'source' : 'package'] = err.consent_token
    const kinds = (['source', 'package'] as AgreementKind[]).filter((k) => at.accepted[k])
    const consent = { kinds, tokens: kinds.map((k) => at.accepted[k] as string) }
    if (at.operation === 'remove') {
      return this.start(id, 'remove', () => api.startUninstall(id, { consent }))
    }
    const options: InstallOptions = {
      ...at.options,
      version: err.version ?? at.options?.version,
      consent,
    }
    if (!options.version) delete options.version
    return this.start(id, 'install', () => api.startInstall(id, options))
  }

  /** "Cancel" in the consent dialog (SPEC §6.6 step 6): no call, the attempt ends. */
  declineAgreement(id: PackageId) {
    this.attempts.delete(id)
    delete this.byPackage[id]
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
      agreement: null,
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
