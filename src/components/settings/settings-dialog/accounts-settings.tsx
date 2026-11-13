import * as React from 'react'
import { Check, Plus, Trash2, UserRound } from 'lucide-react'
import { toast } from 'sonner'

import { Button } from '@renderer/components/ui/button'
import { Input } from '@renderer/components/ui/input'
import type { MailAccount } from '@renderer/shared/types'
import { SettingsGroup, SETTINGS_LIST_CLASS } from './settings-ui'

/**
 * 账号管理：列出全部邮箱，支持添加、移除、改备注名/短徽标，以及暂停同步。
 *
 * 之前设置里没有账号入口，加完账号只能回到主界面碰运气；这里给一个集中的管理位。
 */
export function AccountsSettings(): React.JSX.Element {
  const [accounts, setAccounts] = React.useState<MailAccount[]>([])
  const [loading, setLoading] = React.useState(true)
  const [busyId, setBusyId] = React.useState<number | null>(null)
  const [editingId, setEditingId] = React.useState<number | null>(null)
  const [draftLabel, setDraftLabel] = React.useState('')
  const [draftBadge, setDraftBadge] = React.useState('')
  const [draftColor, setDraftColor] = React.useState('#2d70d2')

  const reload = React.useCallback(async () => {
    try {
      setAccounts(await window.api.accounts.list())
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '读取账号失败')
    } finally {
      setLoading(false)
    }
  }, [])

  React.useEffect(() => {
    void reload()
  }, [reload])

  async function toggleSync(account: MailAccount): Promise<void> {
    setBusyId(account.accountId)
    try {
      await window.api.accounts.update({
        accountId: account.accountId,
        syncEnabled: !account.syncEnabled
      })
      await reload()
      toast.success(account.syncEnabled ? '已暂停该账号的同步' : '已恢复同步')
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '更新失败')
    } finally {
      setBusyId(null)
    }
  }

  function beginEdit(account: MailAccount): void {
    setEditingId(account.accountId)
    setDraftLabel(account.accountLabel ?? '')
    setDraftBadge(account.badgeLabel ?? '')
    setDraftColor(account.badgeColor ?? '#2d70d2')
  }

  async function saveEdit(account: MailAccount): Promise<void> {
    setBusyId(account.accountId)
    try {
      await window.api.accounts.update({
        accountId: account.accountId,
        accountLabel: draftLabel.trim() || undefined,
        badgeLabel: draftBadge.trim().slice(0, 10) || null,
        badgeColor: draftColor
      })
      await reload()
      setEditingId(null)
      toast.success('账号信息已保存')
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '保存失败')
    } finally {
      setBusyId(null)
    }
  }

  async function removeAccount(account: MailAccount): Promise<void> {
    if (!window.confirm(`确定移除 ${account.email}？本地缓存的邮件会一并删除。`)) return
    setBusyId(account.accountId)
    try {
      await window.api.accounts.remove(account.accountId)
      await reload()
      toast.success('账号已移除')
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '移除失败')
    } finally {
      setBusyId(null)
    }
  }

  return (
    <SettingsGroup title="账号管理">
      <div className={SETTINGS_LIST_CLASS}>
        <div className="divide-y divide-border">
          {loading ? (
            <p className="p-3 text-[11.5px] text-muted-foreground">正在载入账号…</p>
          ) : accounts.length === 0 ? (
            <p className="p-3 text-[11.5px] text-muted-foreground">还没有添加邮箱账号。</p>
          ) : (
            accounts.map((account) => (
              <div key={account.accountId} className="space-y-2 p-3">
                <div className="flex items-start gap-2">
                <span className="mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-full bg-primary/10 text-primary">
                  <UserRound className="size-3.5" aria-hidden="true" />
                </span>
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[12.5px] font-medium">
                    {account.accountLabel || account.email}
                  </p>
                  <p className="truncate text-[11px] text-muted-foreground">{account.email}</p>
                  <p className="mt-0.5 text-[10.5px] text-muted-foreground">
                    {account.syncEnabled ? '同步已开启' : '同步已暂停'}
                    {account.status ? ` · ${account.status}` : ''}
                  </p>
                </div>
                <div className="flex shrink-0 items-center gap-1">
                  <span
                    className="rounded-[4px] px-1 text-[9px] font-bold leading-4 text-white"
                    style={{ backgroundColor: account.badgeColor ?? '#2d70d2' }}
                    title={account.badgeColor ?? ''}
                  >
                    {account.badgeLabel || '—'}
                  </span>
                  {editingId === account.accountId ? (
                    <Button
                      type="button"
                      size="sm"
                      disabled={busyId === account.accountId}
                      onClick={() => void saveEdit(account)}
                    >
                      <Check className="size-3.5" aria-hidden="true" />
                    </Button>
                  ) : (
                    <Button type="button" size="sm" variant="ghost" onClick={() => beginEdit(account)}>
                      编辑
                    </Button>
                  )}
                  <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    disabled={busyId === account.accountId}
                    onClick={() => void toggleSync(account)}
                  >
                    {account.syncEnabled ? '暂停同步' : '恢复同步'}
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    className="text-destructive"
                    disabled={busyId === account.accountId}
                    onClick={() => void removeAccount(account)}
                  >
                    <Trash2 className="size-3.5" aria-hidden="true" />
                  </Button>
                </div>
                </div>
                {editingId === account.accountId ? (
                  <div className="mt-2 w-full space-y-2 rounded-lg border border-border bg-background/60 p-2">
                    <div className="grid grid-cols-2 gap-2">
                      <label className="text-[11px]">
                        备注名
                        <Input
                          value={draftLabel}
                          onChange={(event) => setDraftLabel(event.target.value)}
                          placeholder="例如：工作邮箱"
                          className="mt-1 h-7 text-[11.5px]"
                        />
                      </label>
                      <label className="text-[11px]">
                        短徽标（≤10 字，悬浮显示）
                        <Input
                          value={draftBadge}
                          maxLength={10}
                          onChange={(event) => setDraftBadge(event.target.value)}
                          placeholder="G / QQ / 工作邮箱"
                          className="mt-1 h-7 text-[11.5px]"
                        />
                      </label>
                    </div>
                    <label className="flex items-center gap-2 text-[11px]">
                      徽标颜色
                      <input
                        type="color"
                        value={draftColor}
                        onChange={(event) => setDraftColor(event.target.value)}
                        className="h-6 w-10 cursor-pointer rounded border border-border bg-transparent"
                      />
                      <span className="font-mono text-[10.5px] text-muted-foreground">{draftColor}</span>
                    </label>
                  </div>
                ) : null}
              </div>
            ))
          )}
        </div>
        <div className="border-t border-border p-3">
          <Button
            type="button"
            size="sm"
            onClick={() => {
              void window.api.accounts.openAddWindow().catch((error: unknown) => {
                toast.error(error instanceof Error ? error.message : '打开添加账号窗口失败')
              })
            }}
          >
            <Plus className="size-3.5" aria-hidden="true" /> 添加邮箱账号
          </Button>
          <p className="mt-2 text-[11px] text-muted-foreground">
            账号的短徽标与颜色可在「通用」里调整；暂停同步后该账号不会自动拉取新邮件。
          </p>
        </div>
      </div>
    </SettingsGroup>
  )
}
