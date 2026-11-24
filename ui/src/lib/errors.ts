import type { AppError } from './types'

/** SPEC §6.6, case (c): the shown text when winget needs an agreement the app cannot show. */
export const NO_AGREEMENT_TEXT =
  'The agreement text is not available. Store cannot install this package.'

/** Text for an error. Reads `error.code`, never the message text. */
export function errorText(e: AppError): string {
  return e.code === 'agreements_required' ? NO_AGREEMENT_TEXT : e.message
}

/** The agreement link of an `agreements_required` error, to show as plain text; else null. */
export function agreementUrl(e: AppError): string | null {
  return e.code === 'agreements_required' && e.agreement_url ? e.agreement_url : null
}

/** The rule of `validate_version` in src-tauri/src/ipc/commands.rs. */
export function isValidVersion(v: string): boolean {
  return v.length >= 1 && v.length <= 64 && !v.startsWith('-') && /^[0-9A-Za-z._+-]+$/.test(v)
}
