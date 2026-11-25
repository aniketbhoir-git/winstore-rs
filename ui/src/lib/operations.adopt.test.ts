// L2 tests (docs/designs/unattended-testing.md): a reload restores the "cancel failed" state.
import { beforeAll, beforeEach, describe, expect, it } from 'vitest'
import { installBridge } from '../test/ipc'
import { operations } from './operations.svelte'

const id = 'winget:Foo.Bar'

describe('operations.adopt', () => {
  beforeAll(async () => {
    installBridge()
    await operations.init()
  })
  beforeEach(() => {
    operations.byPackage = {}
  })

  it('takes cancel_failed from the active-operation status', () => {
    operations.adopt(id, { operation_id: 'op-1', operation: 'install', cancel_failed: true })
    expect(operations.get(id)?.cancelFailed).toBe(true)
  })

  it('starts with Cancel enabled when the backend reports no failed cancel', () => {
    operations.adopt(id, { operation_id: 'op-1', operation: 'install', cancel_failed: false })
    expect(operations.get(id)?.cancelFailed).toBe(false)
  })

  it('a later status of the same operation sets the flag and never clears it', () => {
    operations.adopt(id, { operation_id: 'op-1', operation: 'install', cancel_failed: false })
    operations.adopt(id, { operation_id: 'op-1', operation: 'install', cancel_failed: true })
    expect(operations.get(id)?.cancelFailed).toBe(true)
    operations.adopt(id, { operation_id: 'op-1', operation: 'install', cancel_failed: false })
    expect(operations.get(id)?.cancelFailed).toBe(true)
  })
})
