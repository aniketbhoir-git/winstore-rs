// Mirrors the Rust domain/IPC types (src-tauri/src/{domain,ipc}). Keep in sync by hand.

export type SourceKind = 'winget_community' | 'ms_store' | 'scoop' | 'chocolatey'
export type PackagingKind = 'msix' | 'msix_bundle' | 'exe' | 'msi' | 'portable' | 'unknown'
export type OperationStage =
  | 'queued'
  | 'resolving'
  | 'downloading'
  | 'installing'
  | 'finalizing'
  | 'done'
  | 'failed'
  | 'cancelled'
export type OperationKind = 'install' | 'remove' | 'update'
export type SortOrder = 'relevance' | 'name' | 'rating'

/** `{source}/{package_id}` */
export type PackageId = string

export interface PackageSummary {
  id: PackageId
  name: string
  publisher: string
  latest_version: string
  description: string
  icon_url: string | null
  rating: number | null
  rating_count: number | null
  source: SourceKind
  is_paid: boolean
  packaging: PackagingKind
}

export interface PackageVersion {
  version: string
  release_date: string | null
  installer_url: string | null
  sha256: string | null
}

export interface PackageDetail {
  summary: PackageSummary
  full_description: string
  screenshots: string[]
  website: string | null
  license: string | null
  release_notes: string | null
  installer: {
    architecture: string | null
    install_type: PackagingKind
    size_bytes: number | null
    sha256: string | null
  }
  versions: Record<string, PackageVersion>
}

export interface SearchQuery {
  text: string
  category?: string | null
  source?: SourceKind | null
  sort?: SortOrder
  page?: number
  page_size?: number
}

export interface UpdateAvailable {
  package_id: PackageId
  installed_version: string
  latest_version: string
  source: SourceKind
  is_auto: boolean
}

/** What every failed command and failure event carries. */
export interface AppError {
  code: string
  message: string
  detail: string | null
  retryable: boolean
}

export interface ProgressEvent {
  operation_id: string
  package_id: PackageId
  operation: OperationKind
  stage: OperationStage
  fraction: number
  message: string
  timestamp: number
}

export interface SearchResultsEvent {
  search_id: string
  results: PackageSummary[]
}

export interface SearchFailedEvent {
  search_id: string
  error: AppError
}

export interface OperationFinishedEvent {
  operation_id: string
  package_id: PackageId
  ok: boolean
  error: AppError | null
}

export interface UpdatesSummaryEvent {
  total: number
  succeeded: number
  failed: number
  outcomes: { package_id: PackageId; ok: boolean; message: string | null }[]
}

export interface InstallOptions {
  version?: string
  scope?: 'user' | 'machine'
}

export type InstallState =
  | 'not_installed'
  | 'installing'
  | 'installed'
  | 'update_available'
  | 'uninstalling'
  | 'failed'
  | 'pinned'

export interface ActiveOperation {
  operation_id: string
  operation: OperationKind
}

/** Ledger state for one package plus the operation running for it, if any. */
export interface InstallStatus {
  package_id: PackageId
  state: InstallState
  installed_version: string | null
  target_version: string | null
  pinned: boolean
  last_updated: number | null
  active: ActiveOperation | null
}

export type LogLevel = 'debug' | 'info' | 'warn' | 'error'

export interface OperationLogEntry {
  id: number | null
  ts: number
  level: LogLevel
  operation: OperationKind | null
  package_id: PackageId | null
  message: string
  detail: string | null
}
