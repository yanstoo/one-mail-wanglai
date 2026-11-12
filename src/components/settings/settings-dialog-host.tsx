import * as React from 'react'

import { SettingsDialog } from './settings-dialog'
import {
  clearAiSettings,
  getAppUpdateStatus,
  loadAiSettings,
  onAppUpdateStatus,
  saveSettings,
  verifyAndSaveAiSettings
} from '@renderer/lib/api'
import { normalizeLocale, useI18n } from '@renderer/lib/i18n'
import type {
  AiSettings,
  AppSettings,
  AppUpdateStatus,
  SettingsUpdateInput,
  SystemInfo,
  TranslateSettings,
  TranslateSettingsInput
} from '@renderer/shared/types'

/**
 * 在主窗口内以弹窗形式打开设置。
 *
 * 之前设置是独立 Tauri 窗口：需要单独建 WebView、关闭按钮在小窗口里不好点，
 * 还要靠预热掩盖创建开销。改成主窗口内的模态弹窗后，没有窗口管理问题，
 * 关闭就是点遮罩或右上角。
 */
export function SettingsDialogHost({
  open,
  onClose
}: {
  open: boolean
  onClose: () => void
}): React.JSX.Element | null {
  const { setLocale } = useI18n()
  const [settings, setSettings] = React.useState<AppSettings | null>(null)
  const [aiSettings, setAiSettings] = React.useState<AiSettings | null>(null)
  const [translateSettings, setTranslateSettings] = React.useState<TranslateSettings | null>(null)
  const [systemInfo, setSystemInfo] = React.useState<SystemInfo | null>(null)
  const [updateStatus, setUpdateStatus] = React.useState<AppUpdateStatus | null>(null)
  const [error, setError] = React.useState<string | null>(null)

  React.useEffect(() => {
    if (!open) return
    let active = true
    void Promise.all([
      window.api.settings.get(),
      loadAiSettings().catch(() => null),
      window.api.system.info(),
      getAppUpdateStatus().catch(() => null),
      window.api.translate.getSettings().catch(() => null)
    ])
      .then(([nextSettings, nextAi, nextSystem, nextUpdate, nextTranslate]) => {
        if (!active) return
        setSettings(nextSettings)
        setAiSettings(nextAi)
        setSystemInfo(nextSystem)
        setUpdateStatus(nextUpdate)
        setTranslateSettings(nextTranslate)
        setLocale(normalizeLocale(nextSettings.locale))
        setError(null)
      })
      .catch((reason: unknown) => {
        if (active) setError(reason instanceof Error ? reason.message : String(reason))
      })
    const stopStatus = onAppUpdateStatus(setUpdateStatus)
    return () => {
      active = false
      stopStatus()
    }
  }, [open, setLocale])

  if (!open) return null

  return (
    <>
      <div className="fixed inset-0 z-[70] bg-black/40" onClick={onClose} aria-hidden="true" />
      <div className="fixed inset-0 z-[71] flex items-center justify-center p-6" role="dialog" aria-modal="true">
        <div className="h-[min(560px,85vh)] w-[min(720px,92vw)] overflow-hidden rounded-xl border border-border bg-muted shadow-2xl">
          {error ? (
            <p role="alert" className="p-5 text-sm text-destructive">
              {error}
            </p>
          ) : settings ? (
            <SettingsDialog
              open
              standalone
              settings={settings}
              systemInfo={systemInfo}
              updateStatus={updateStatus}
              aiSettings={aiSettings}
              translateSettings={translateSettings}
              onOpenChange={(next) => {
                if (!next) onClose()
              }}
              onSubmit={async (input: SettingsUpdateInput) => {
                const updated = await saveSettings(input)
                setSettings(updated)
                setLocale(normalizeLocale(updated.locale))
              }}
              onVerifyAi={async (input) => {
                const updated = await verifyAndSaveAiSettings(input)
                setAiSettings(updated)
                return updated
              }}
              onClearAi={async () => {
                const updated = await clearAiSettings()
                setAiSettings(updated)
                return updated
              }}
              onSaveTranslate={async (input: TranslateSettingsInput) => {
                const saved = await window.api.translate.saveSettings(input)
                setTranslateSettings(saved)
                return saved
              }}
            />
          ) : (
            <p className="p-5 text-sm text-muted-foreground">正在载入设置…</p>
          )}
        </div>
      </div>
    </>
  )
}
