// Shared helpers for L2 tests: a recording IPC mock and the shared Rust fixtures.
import { mockIPC } from '@tauri-apps/api/mocks'

export { default as errorAgreementsRequired } from '../../../src-tauri/tests/fixtures/ipc/error_agreements_required.json'
export { default as eventProgress } from '../../../src-tauri/tests/fixtures/ipc/event_progress.json'
export { default as eventOperationFinished } from '../../../src-tauri/tests/fixtures/ipc/event_operation_finished.json'

export interface Call {
  cmd: string
  args: unknown
}

type Handlers = Record<string, (args: any) => unknown>

let handlers: Handlers = {}
let calls: Call[] = []

/**
 * Installs the fake bridge once per test file (a second `mockIPC` call would drop the event
 * listeners that app singletons registered). Event plugin traffic is not a backend command.
 */
export function installBridge() {
  mockIPC(
    (cmd, args) => {
      if (cmd.startsWith('plugin:event|')) return undefined
      calls.push({ cmd, args })
      const h = handlers[cmd]
      if (!h) throw new Error(`unmocked command: ${cmd}`)
      return h(args)
    },
    { shouldMockEvents: true },
  )
}

/** Sets this test's command replies (a handler may throw) and returns a fresh call log. */
export function mockBackend(next: Handlers = {}): Call[] {
  handlers = next
  calls = []
  return calls
}
