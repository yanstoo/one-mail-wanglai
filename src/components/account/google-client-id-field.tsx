import * as React from 'react'
import { ExternalLink } from 'lucide-react'
import { toast } from 'sonner'

import { Button } from '@renderer/components/ui/button'
import { Input } from '@renderer/components/ui/input'
import { useI18n } from '@renderer/lib/i18n'

const GOOGLE_CREDENTIALS_URL = 'https://console.cloud.google.com/apis/credentials'

/**
 * Gmail 走 OAuth 需要用户自己的 Google 桌面客户端 ID。
 * 上游只读环境变量，包装成桌面应用后用户无从配置，这里提供应用内入口。
 */
export function GoogleClientIdField(): React.JSX.Element {
  const { t } = useI18n()
  const [value, setValue] = React.useState('')
  const [source, setSource] = React.useState<'env' | 'stored' | 'none'>('none')
  const [loading, setLoading] = React.useState(true)
  const [saving, setSaving] = React.useState(false)

  React.useEffect(() => {
    let cancelled = false
    void window.api.system
      .getGoogleClientId()
      .then((result) => {
        if (cancelled) return
        setValue(result.clientId ?? '')
        setSource(result.source)
      })
      .catch(() => null)
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [])

  async function persist(next: string | null): Promise<void> {
    setSaving(true)
    try {
      const result = await window.api.system.setGoogleClientId(next)
      setValue(result.clientId ?? '')
      setSource(result.clientId ? 'stored' : 'none')
      toast.success(result.clientId ? t('account.form.googleClientIdSaved') : t('account.form.googleClientIdClear'))
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error))
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="space-y-2 rounded-lg border border-dashed border-border bg-muted/30 p-3">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-semibold">{t('account.form.googleClientId')}</span>
        <button
          type="button"
          onClick={() => void window.api.system.openExternal(GOOGLE_CREDENTIALS_URL).catch(() => null)}
          className="flex items-center gap-1 text-[11px] text-primary hover:underline"
        >
          {t('account.form.openGoogleConsole')}
          <ExternalLink className="size-3" aria-hidden="true" />
        </button>
      </div>
      <p className="text-[11px] leading-5 text-muted-foreground">{t('account.form.googleClientIdHelp')}</p>
      <div className="flex items-center gap-2">
        <Input
          value={value}
          onChange={(event) => setValue(event.target.value)}
          placeholder={t('account.form.googleClientIdPlaceholder')}
          autoComplete="off"
          spellCheck={false}
          disabled={loading || saving || source === 'env'}
          className="font-mono text-[11px]"
        />
        <Button
          type="button"
          size="sm"
          disabled={loading || saving || source === 'env' || value.trim().length === 0}
          onClick={() => void persist(value.trim())}
        >
          {t('account.form.googleClientIdSave')}
        </Button>
        {source === 'stored' ? (
          <Button type="button" size="sm" variant="ghost" disabled={saving} onClick={() => void persist(null)}>
            {t('account.form.googleClientIdClear')}
          </Button>
        ) : null}
      </div>
      {source === 'env' ? (
        <p className="text-[10.5px] text-muted-foreground">已由环境变量 ONEMAIL_GOOGLE_CLIENT_ID 提供</p>
      ) : null}
    </div>
  )
}
