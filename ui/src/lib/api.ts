// Typed client for the Tauri command / event surface (src-tauri/src/ipc).
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type {
  AppError,
  InstallOptions,
  OperationFinishedEvent,
  PackageDetail,
  PackageSummary,
  ProgressEvent,
  SearchFailedEvent,
  SearchQuery,
  SearchResultsEvent,
  UpdateAvailable,
  UpdatesSummaryEvent,
} from './types'

/** Event names; must match src-tauri/src/ipc/events.rs. */
export const EVENTS = {
  progress: 'store:progress',
  searchResults: 'store:search-results',
  searchFailed: 'store:search-failed',
  operationFinished: 'store:operation-finished',
  updatesSummary: 'store:updates-summary',
} as const

export function toAppError(e: unknown): AppError {
  if (e && typeof e === 'object' && 'code' in e && 'message' in e) return e as AppError
  return {
    code: 'unknown',
    message: typeof e === 'string' ? e : 'Something went wrong.',
    detail: null,
    retryable: false,
  }
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args)
  } catch (e) {
    throw toAppError(e)
  }
}

export const api = {
  appVersion: () => call<string>('app_version'),
  searchSources: (searchId: string, query: SearchQuery) =>
    call<PackageSummary[]>('search_sources', { searchId, query }),
  getAppDetail: (packageId: string) => call<PackageDetail>('get_app_detail', { packageId }),
  startInstall: (packageId: string, options?: InstallOptions) =>
    call<{ operation_id: string }>('start_install', { packageId, options }),
  cancelInstall: (operationId: string) => call<void>('cancel_install', { operationId }),
  checkUpdates: () => call<UpdateAvailable[]>('check_updates'),
  updateAll: () => call<UpdatesSummaryEvent>('update_all'),
  openStorePage: (productId: string) => call<void>('open_store_page', { productId }),
  startServer: (port: number) => call<void>('start_server', { port }),
}

function subscribe<T>(name: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(name, (e) => handler(e.payload))
}

export const events = {
  onProgress: (h: (e: ProgressEvent) => void) => subscribe(EVENTS.progress, h),
  onSearchResults: (h: (e: SearchResultsEvent) => void) => subscribe(EVENTS.searchResults, h),
  onSearchFailed: (h: (e: SearchFailedEvent) => void) => subscribe(EVENTS.searchFailed, h),
  onOperationFinished: (h: (e: OperationFinishedEvent) => void) =>
    subscribe(EVENTS.operationFinished, h),
  onUpdatesSummary: (h: (e: UpdatesSummaryEvent) => void) => subscribe(EVENTS.updatesSummary, h),
}
