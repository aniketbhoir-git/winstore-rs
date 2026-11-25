import type { AppError } from './types'

/** SPEC §6.6 "no text" rule: the shown text when winget needs an agreement the app cannot show. */
export const NO_AGREEMENT_TEXT =
  'The agreement text is not available. Store cannot install this package.'

/** Hardening design §1 rule 6: the text for `elevation_required`. */
export const ELEVATION_REQUIRED_TEXT =
  "This app needs administrator rights to install. Windows did not give them. Try again and accept the Windows prompt, or choose 'Only for me' if the app supports it."

/** Hardening design §1 "Cancel of an elevated installer": shown when `cancel_failed` is set. */
export const CANCEL_FAILED_TEXT = 'The installer runs with administrator rights. Store cannot stop it.'

/** Text for an error. Reads `error.code`, never the message text. */
export function errorText(e: AppError): string {
  if (e.code === 'agreements_required') return NO_AGREEMENT_TEXT
  if (e.code === 'elevation_required') return ELEVATION_REQUIRED_TEXT
  return e.message
}

/**
 * Design catalog-winget-cli §6, point 5: text for `agreements_required` on a read-only call
 * (search, detail). No consent dialog; the terms are accepted in a terminal.
 */
export const READ_ONLY_AGREEMENT_TEXT =
  'winget needs you to accept the terms of the winget source. Run `winget search` once in a terminal, read the terms, and accept them there.'

/** Text for an error of a read-only call (search, detail). Reads `error.code` only. */
export function readOnlyErrorText(e: AppError): string {
  return e.code === 'agreements_required' ? READ_ONLY_AGREEMENT_TEXT : e.message
}

/**
 * SPEC §6.6 step 3: an `agreements_required` error opens the consent dialog only when it has
 * something to show (a text or a link) and a token that a consent can carry. Reads
 * `error.code` and the fields, never the message text.
 */
export function hasAgreementDialog(e: AppError): boolean {
  if (e.code !== 'agreements_required' || !e.consent_token) return false
  return (
    !!e.agreement_text ||
    !!e.agreement_url ||
    (e.agreements ?? []).some((a) => !!a.text || !!a.url)
  )
}

/** The agreement link of an `agreements_required` error, to show as plain text; else null. */
export function agreementUrl(e: AppError): string | null {
  return e.code === 'agreements_required' && e.agreement_url ? e.agreement_url : null
}

/** The rule of `validate_version` in src-tauri/src/ipc/commands.rs. */
export function isValidVersion(v: string): boolean {
  return v.length >= 1 && v.length <= 64 && !v.startsWith('-') && /^[0-9A-Za-z._+-]+$/.test(v)
}
