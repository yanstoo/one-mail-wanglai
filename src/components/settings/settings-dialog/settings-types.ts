export type BackupPending =
  | 'export'
  | 'import'
  | 'saveRemote'
  | 'testRemote'
  | 'uploadRemote'
  | 'downloadRemote'
  | null
export type BackupMessage = {
  label: string
  path?: string
}
export type TranslationSettingsProps = {
  settings: import('@renderer/shared/types').TranslateSettings | null
  onSave: (
    input: import('@renderer/shared/types').TranslateSettingsInput
  ) => Promise<import('@renderer/shared/types').TranslateSettings>
}
