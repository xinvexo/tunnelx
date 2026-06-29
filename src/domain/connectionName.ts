export const CONNECTION_NAME_MAX_LENGTH = 64

const VALID_CONNECTION_NAME = /^[\p{L}\p{N}._-]+$/u

export type ConnectionNameValidation =
  | { ok: true; name: string }
  | { ok: false; messageKey: 'connectionName.required' | 'connectionName.invalid' | 'connectionName.tooLong' }

export function validateConnectionName(value: string): ConnectionNameValidation {
  const name = value.trim()
  if (!name) return { ok: false, messageKey: 'connectionName.required' }
  if (Array.from(name).length > CONNECTION_NAME_MAX_LENGTH) return { ok: false, messageKey: 'connectionName.tooLong' }
  if (name !== value || !VALID_CONNECTION_NAME.test(name)) return { ok: false, messageKey: 'connectionName.invalid' }
  return { ok: true, name }
}
