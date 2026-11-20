import * as React from 'react'
import { Check, Copy, KeyRound, RefreshCw } from 'lucide-react'
import { toast } from 'sonner'

import { Button } from '@renderer/components/ui/button'
import { Input } from '@renderer/components/ui/input'
import type { McpSettings } from '@renderer/shared/types'
import { SettingsGroup, SETTINGS_LIST_CLASS } from './settings-ui'

/**
 * MCP 设置：让外部 AI 客户端（Cursor / Claude Desktop 等）通过本机 HTTP 访问邮箱。
 *
 * 权限分三档：读取始终允许；写草稿需显式开启；**发送默认关闭**，
 * 只有用户自己打开后 AI 才能把草稿真的发出去。
 */
export function McpSettingsPanel(): React.JSX.Element {
  const [settings, setSettings] = React.useState<McpSettings | null>(null)
  const [port, setPort] = React.useState('8787')
  const [pending, setPending] = React.useState(false)
  const [copied, setCopied] = React.useState<'token' | 'config' | null>(null)

  const load = React.useCallback(async () => {
    try {
      const next = await window.api.mcp.getSettings()
      setSettings(next)
      setPort(String(next.port))
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error))
    }
  }, [])

  React.useEffect(() => {
    void load()
  }, [load])

  async function save(patch: Parameters<typeof window.api.mcp.saveSettings>[0]): Promise<void> {
    setPending(true)
    try {
      const next = await window.api.mcp.saveSettings(patch)
      setSettings(next)
      setPort(String(next.port))
    } catch (error) {
      toast.error(typeof error === 'string' ? error : String(error))
    } finally {
      setPending(false)
    }
  }

  async function copy(text: string, kind: 'token' | 'config'): Promise<void> {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(kind)
      window.setTimeout(() => setCopied(null), 1500)
    } catch {
      toast.error('复制失败，请手动选择文本')
    }
  }

  const config = settings
    ? JSON.stringify(
        {
          mcpServers: {
            'one-mail-wanglai': {
              type: 'http',
              url: settings.url,
              headers: { Authorization: `Bearer ${settings.token}` }
            }
          }
        },
        null,
        2
      )
    : ''

  return (
    <SettingsGroup title="MCP（外部 AI 访问）">
      <div className={`${SETTINGS_LIST_CLASS} divide-y divide-border`}>
        <div className="flex items-start justify-between gap-3 p-3">
          <div className="min-w-0">
            <p className="text-[12px] font-medium">启用 MCP 服务</p>
            <p className="mt-0.5 text-[11px] leading-4 text-muted-foreground">
              只监听本机 127.0.0.1，外部设备无法直连。开启后 AI 客户端可通过下面的地址读取邮件、写草稿。
            </p>
            <p className="mt-1 text-[10.5px] text-muted-foreground">
              状态：{settings?.running ? `运行中 · ${settings.url}` : '未运行'}
            </p>
          </div>
          <label className="flex shrink-0 cursor-pointer items-center gap-2">
            <input
              type="checkbox"
              checked={settings?.enabled ?? false}
              disabled={pending || !settings}
              onChange={(event) => void save({ enabled: event.target.checked })}
              className="size-4 accent-[var(--primary)]"
            />
          </label>
        </div>

        <div className="p-3">
          <p className="text-[12px] font-medium">端口</p>
          <div className="mt-1.5 flex items-center gap-2">
            <Input
              value={port}
              onChange={(event) => setPort(event.target.value.replace(/[^0-9]/g, ''))}
              className="h-8 w-24 text-[11.5px]"
              aria-label="MCP 端口"
            />
            <Button
              type="button"
              size="sm"
              variant="ghost"
              disabled={pending}
              onClick={() => void save({ port: Number(port) })}
            >
              应用
            </Button>
            <span className="text-[10.5px] text-muted-foreground">默认 8787，范围 1024–65535</span>
          </div>
        </div>

        <div className="p-3">
          <p className="text-[12px] font-medium">访问令牌</p>
          <p className="mt-0.5 text-[11px] text-muted-foreground">
            客户端需带 <code>Authorization: Bearer &lt;令牌&gt;</code>。令牌只保存在本机数据库。
          </p>
          <div className="mt-1.5 flex items-center gap-2">
            <Input
              readOnly
              value={settings?.token ?? ''}
              className="h-8 flex-1 font-mono text-[10.5px]"
              aria-label="MCP 访问令牌"
              onFocus={(event) => event.currentTarget.select()}
            />
            <Button
              type="button"
              size="sm"
              variant="ghost"
              onClick={() => void copy(settings?.token ?? '', 'token')}
            >
              {copied === 'token' ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
              复制
            </Button>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              disabled={pending}
              title="重新生成后旧令牌立即失效"
              onClick={() => void save({ regenerateToken: true })}
            >
              <RefreshCw className="size-3.5" />
              重置
            </Button>
          </div>
        </div>

        <div className="space-y-2 p-3">
          <p className="text-[12px] font-medium">权限</p>
          <label className="flex items-start gap-2 text-[11.5px]">
            <input type="checkbox" checked readOnly className="mt-0.5 size-3.5 accent-[var(--primary)]" />
            <span>
              读取邮件与状态<span className="text-muted-foreground">（启用即允许，不可关闭）</span>
            </span>
          </label>
          <label className="flex items-start gap-2 text-[11.5px]">
            <input
              type="checkbox"
              checked={settings?.allowDraft ?? true}
              disabled={pending || !settings}
              onChange={(event) => void save({ allowDraft: event.target.checked })}
              className="mt-0.5 size-3.5 accent-[var(--primary)]"
            />
            <span>
              写草稿<span className="text-muted-foreground">（新建、列出、删除草稿）</span>
            </span>
          </label>
          <label className="flex items-start gap-2 text-[11.5px]">
            <input
              type="checkbox"
              checked={settings?.allowSend ?? false}
              disabled={pending || !settings}
              onChange={(event) => void save({ allowSend: event.target.checked })}
              className="mt-0.5 size-3.5 accent-[var(--primary)]"
            />
            <span>
              发送邮件
              <span className="text-muted-foreground">
                （默认关闭；打开后 AI 可把草稿直接发出，建议保持关闭并人工确认）
              </span>
            </span>
          </label>
        </div>

        <div className="p-3">
          <div className="flex items-center justify-between gap-2">
            <p className="flex items-center gap-1 text-[12px] font-medium">
              <KeyRound className="size-3.5" aria-hidden="true" />
              客户端配置
            </p>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              disabled={!settings}
              onClick={() => void copy(config, 'config')}
            >
              {copied === 'config' ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
              复制 JSON
            </Button>
          </div>
          <pre className="mt-1.5 max-h-40 overflow-auto rounded-md bg-black/5 p-2 text-[10.5px] leading-4 dark:bg-white/10">
            {config || '（启用后生成）'}
          </pre>
          <p className="mt-1 text-[10.5px] text-muted-foreground">
            提供的工具：list_accounts、mail_status、list_correspondents、list_messages、get_message、
            create_draft、list_drafts、delete_draft、send_draft（需开启发送权限）。
          </p>
        </div>
      </div>
    </SettingsGroup>
  )
}
