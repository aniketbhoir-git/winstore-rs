// A request to review a file in the exports folder (from the export panel to the import panel).
// A counter makes a second request for the same path load again.
export const importRequest = $state<{ path: string | null; n: number }>({ path: null, n: 0 })

export function requestImport(path: string) {
  importRequest.path = path
  importRequest.n += 1
}
