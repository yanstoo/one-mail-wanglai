import * as React from 'react'
import { Button } from '@renderer/components/ui/button'
import { Field, FieldGroup, FieldLabel } from '@renderer/components/ui/field'
import { Input } from '@renderer/components/ui/input'
import type {
  TranslateBackend,
  TranslateSettings,
  TranslateSettingsInput
} from '@renderer/shared/types'
import { SettingsGroup, SETTINGS_LIST_CLASS } from './settings-ui'

const BACKENDS: Array<{ value: TranslateBackend; label: string }> = [
  { value: 'google', label: 'Google 免费端点（零配置）' },
  { value: 'libretranslate', label: 'LibreTranslate（开源，可自托管）' },
  { value: 'azure', label: 'Azure Translator（需密钥）' },
  { value: 'deepl', label: 'DeepL（需密钥）' }
]

const TARGET_LANGUAGES = [
  { value: '', label: '简体中文（默认）' },
  { value: 'zh-TW', label: '繁體中文' },
  { value: 'en', label: 'English' },
  { value: 'ja', label: '日本語' },
  { value: 'ko', label: '한국어' }
]

export function TranslationSettings({
  settings,
  onSave
}: {
  settings: TranslateSettings | null
  onSave: (input: TranslateSettingsInput) => Promise<TranslateSettings>
}): React.JSX.Element {
  const [backend, setBackend] = React.useState<TranslateBackend>(settings?.backend ?? 'google')
  const [targetLang, setTargetLang] = React.useState(settings?.targetLang ?? '')
  const [libreUrl, setLibreUrl] = React.useState(settings?.libretranslateUrl ?? '')
  const [apiKey, setApiKey] = React.useState('')
  const [apiRegion, setApiRegion] = React.useState(settings?.apiRegion ?? '')
  const [pending, setPending] = React.useState(false)
  const [message, setMessage] = React.useState<string | null>(null)
  const [error, setError] = React.useState<string | null>(null)

  React.useEffect(() => {
    setBackend(settings?.backend ?? 'google')
    setTargetLang(settings?.targetLang ?? '')
    setLibreUrl(settings?.libretranslateUrl ?? '')
    setApiRegion(settings?.apiRegion ?? '')
    setApiKey('')
  }, [settings])

  const needsKey = backend === 'azure' || backend === 'deepl'
  const needsRegion = backend === 'azure'

  async function handleSave(): Promise<void> {
    setPending(true)
    setError(null)
    setMessage(null)
    try {
      await onSave({
        backend,
        targetLang,
        libretranslateUrl: backend === 'libretranslate' ? libreUrl.trim() || null : null,
        apiRegion: needsRegion ? apiRegion.trim() || null : null,
        // 只有填了才更新密钥，留空表示不改
        ...(apiKey.trim() ? { apiKey: apiKey.trim() } : {})
      })
      setApiKey('')
      setMessage('已保存')
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setPending(false)
    }
  }

  return (
    <SettingsGroup title="翻译（右栏「翻译」按钮与气泡右键「翻译标题」）">
      <div className={SETTINGS_LIST_CLASS}>
        <FieldGroup className="gap-4 p-3">
          <Field>
            <FieldLabel htmlFor="translate-backend">翻译后端</FieldLabel>
            <select
              id="translate-backend"
              value={backend}
              onChange={(event) => setBackend(event.target.value as TranslateBackend)}
              className="w-full rounded-md border border-border bg-background px-2 py-1.5 text-[12.5px] outline-none focus:border-primary/60"
            >
              {BACKENDS.map((item) => (
                <option key={item.value} value={item.value}>
                  {item.label}
                </option>
              ))}
            </select>
            {backend === 'google' ? (
              <p className="text-[11px] leading-5 text-muted-foreground">
                Google 端点无需配置即可使用，但它不是官方授权接口：可能被限流或随时变更，
                并且<strong>邮件正文会发送到 Google 服务器</strong>。介意隐私请换成 LibreTranslate
                自托管或 Azure / DeepL。
              </p>
            ) : null}
          </Field>

          <Field>
            <FieldLabel htmlFor="translate-target">翻译成</FieldLabel>
            <select
              id="translate-target"
              value={targetLang}
              onChange={(event) => setTargetLang(event.target.value)}
              className="w-full rounded-md border border-border bg-background px-2 py-1.5 text-[12.5px] outline-none focus:border-primary/60"
            >
              {TARGET_LANGUAGES.map((item) => (
                <option key={item.value} value={item.value}>
                  {item.label}
                </option>
              ))}
            </select>
          </Field>

          {backend === 'libretranslate' ? (
            <Field>
              <FieldLabel htmlFor="translate-libre-url">服务地址</FieldLabel>
              <Input
                id="translate-libre-url"
                value={libreUrl}
                onChange={(event) => setLibreUrl(event.target.value)}
                placeholder="http://127.0.0.1:5000"
                spellCheck={false}
              />
              <p className="text-[11px] leading-5 text-muted-foreground">
                自托管最省心：<code className="rounded bg-muted px-1">docker run -p 5000:5000 libretranslate/libretranslate</code>
              </p>
            </Field>
          ) : null}

          {needsKey ? (
            <Field>
              <FieldLabel htmlFor="translate-api-key">
                API 密钥{settings?.apiKeyConfigured ? '（已保存，留空表示不修改）' : ''}
              </FieldLabel>
              <Input
                id="translate-api-key"
                type="password"
                value={apiKey}
                onChange={(event) => setApiKey(event.target.value)}
                placeholder={settings?.apiKeyConfigured ? '••••••••' : '粘贴密钥'}
                autoComplete="off"
                spellCheck={false}
              />
            </Field>
          ) : null}

          {needsRegion ? (
            <Field>
              <FieldLabel htmlFor="translate-api-region">资源区域（region）</FieldLabel>
              <Input
                id="translate-api-region"
                value={apiRegion}
                onChange={(event) => setApiRegion(event.target.value)}
                placeholder="eastasia"
                spellCheck={false}
              />
            </Field>
          ) : null}

          <div className="flex items-center gap-2">
            <Button type="button" size="sm" disabled={pending} onClick={() => void handleSave()}>
              保存翻译设置
            </Button>
            {message ? <span className="text-[11px] text-muted-foreground">{message}</span> : null}
            {error ? <span className="text-[11px] text-destructive">{error}</span> : null}
          </div>
        </FieldGroup>
      </div>
    </SettingsGroup>
  )
}
