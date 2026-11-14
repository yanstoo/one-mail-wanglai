import * as React from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import {
  MailPlus,
  Menu,
  PenLine,
  RefreshCw,
  Search,
  Settings,
  UserPlus,
  X
} from 'lucide-react'
import { toast } from 'sonner'

import { getCurrentWindow } from '@tauri-apps/api/window'
import { MailComposer } from '@renderer/components/mail/mail-composer'
import { SettingsDialogHost } from '@renderer/components/settings/settings-dialog-host'
import { WindowControls } from '@renderer/components/window-controls'
import { createComposeDraft, loadAccounts, saveComposedDraft, sendComposedMessage } from '@renderer/lib/api'
import type { ComposeDraft, ComposeKind, SendMessageInput } from '@renderer/lib/api/types'
import { cn } from '@renderer/lib/utils'
import type {
  CorrespondentGroup,
  CorrespondentSummary,
  CorrespondentThreadMessage,
  MailMessageDetail,
  OutboxMessage
} from '@renderer/shared/types'

import { CorrespondentList } from './correspondent-list'
import { CorrespondentThread } from './correspondent-thread'
import { CorrespondentMessageReader } from './message-reader'
import { useCorrespondentGroupCounts, useCorrespondentList, useCorrespondentThread } from './use-correspondents'

function messageKey(message: CorrespondentThreadMessage): string {
  return `${message.source}:${message.messageId}`
}

/** 顶栏右上角的 ≡ 菜单：添加账号 / 设置。 */
function TopMenu({
  onAddAccount,
  onOpenSettings
}: {
  onAddAccount: () => void
  onOpenSettings: () => void
}): React.JSX.Element {
  const [open, setOpen] = React.useState(false)
  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        className="flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label="菜单"
      >
        <Menu className="size-4" aria-hidden="true" />
      </button>
      {open ? (
        <>
          <button
            type="button"
            aria-label="关闭菜单"
            className="fixed inset-0 z-40 cursor-default"
            onClick={() => setOpen(false)}
          />
          <div
            role="menu"
            className="absolute right-0 top-8 z-50 w-44 overflow-hidden rounded-lg border border-border bg-popover p-1 text-[12px] shadow-lg"
          >
            <button
              type="button"
              role="menuitem"
              className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left hover:bg-muted"
              onClick={() => {
                setOpen(false)
                onAddAccount()
              }}
            >
              <UserPlus className="size-3.5" aria-hidden="true" /> 添加账号
            </button>
            <button
              type="button"
              role="menuitem"
              className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left hover:bg-muted"
              onClick={() => {
                setOpen(false)
                onOpenSettings()
              }}
            >
              <Settings className="size-3.5" aria-hidden="true" /> 设置
            </button>
          </div>
        </>
      ) : null}
    </div>
  )
}

export function CorrespondentsWorkspace(): React.JSX.Element {
  const queryClient = useQueryClient()

  const [group, setGroup] = React.useState<CorrespondentGroup>('normal')
  const [keyword, setKeyword] = React.useState('')
  const deferredKeyword = React.useDeferredValue(keyword)
  const [accountId, setAccountId] = React.useState<number | null>(null)
  const [selectedId, setSelectedId] = React.useState<number | null>(null)
  const [selectedMessage, setSelectedMessage] = React.useState<CorrespondentThreadMessage | null>(null)

  // 切换联系人时必须清空右栏：否则会继续显示上一个联系人的邮件，
  // 出现「左栏是 A，右栏正文却是 B 的邮件」的串台现象
  React.useEffect(() => {
    setSelectedMessage(null)
    setDetail(null)
    setOutboxDetail(null)
    setDetailError(null)
  }, [selectedId])
  const [detail, setDetail] = React.useState<MailMessageDetail | null>(null)
  const [outboxDetail, setOutboxDetail] = React.useState<OutboxMessage | null>(null)
  const [detailLoading, setDetailLoading] = React.useState(false)
  const [detailError, setDetailError] = React.useState<string | null>(null)
  const [settingsOpen, setSettingsOpen] = React.useState(false)
  const [syncing, setSyncing] = React.useState(false)
  const [syncProgress, setSyncProgress] = React.useState<{
    completed: number
    total: number
    folder?: string | null
    accountId?: number
  } | null>(null)

  const [renameTarget, setRenameTarget] = React.useState<CorrespondentSummary | null>(null)
  const [renameValue, setRenameValue] = React.useState('')
  const [renameSaving, setRenameSaving] = React.useState(false)

  const [composeOpen, setComposeOpen] = React.useState(false)
  const [composeDraft, setComposeDraft] = React.useState<ComposeDraft | null>(null)
  const [composePending, setComposePending] = React.useState(false)

  const { counts } = useCorrespondentGroupCounts()
  const list = useCorrespondentList(group, deferredKeyword, accountId)
  const { thread, loading: threadLoading } = useCorrespondentThread(selectedId)
  const accountsQuery = useQuery({ queryKey: ['accounts'], queryFn: loadAccounts, staleTime: 60_000 })
  const accounts = accountsQuery.data ?? []
  const hasAccounts = accounts.some((account) => account.accountId)

  const openAddAccount = (): void => {
    void window.api.accounts.openAddWindow().catch((error: unknown) => {
      toast.error(error instanceof Error ? error.message : '打开添加账号窗口失败')
    })
  }

  // 设置改为主窗口内弹窗：不再单独建 WebView，也避免小窗口关不掉的问题
  const openSettings = (): void => setSettingsOpen(true)

  // 选中联系人变化时清空右侧
  React.useEffect(() => {
    setSelectedMessage(null)
    setDetail(null)
    setDetailError(null)
  }, [selectedId])

  // 正文按需加载
  React.useEffect(() => {
    if (!selectedMessage) return
    if (selectedMessage.source === 'outbox') {
      // 发件箱记录：从 outbox 表取正文与收件人，右栏照常展示（不再只提示去发件箱看）
      setDetail(null)
      setOutboxDetail(null)
      setDetailLoading(true)
      setDetailError(null)
      const outboxId = selectedMessage.messageId
      let cancelled = false
      void window.api.compose
        .listOutbox({ limit: 200 })
        .then((items) => {
          if (cancelled) return
          const found = items.find((item) => item.outboxId === outboxId) ?? null
          setOutboxDetail(found)
          if (!found) setDetailError('这封已发送邮件已不在发件箱中。')
        })
        .catch((cause: unknown) => {
          if (!cancelled) setDetailError(cause instanceof Error ? cause.message : String(cause))
        })
        .finally(() => {
          if (!cancelled) setDetailLoading(false)
        })
      return () => {
        cancelled = true
      }
    }
    setOutboxDetail(null)
    let cancelled = false
    setDetailLoading(true)
    setDetailError(null)
    const messageId = selectedMessage.messageId
    // 正文在 message_bodies 里，messages.get 只返回摘要；
    // 之前把 loadBody 的结果丢掉了，所以右栏永远显示「正文尚未缓存」
    void Promise.all([
      window.api.messages.get(messageId),
      window.api.messages
        .loadBody(messageId)
        .catch((cause: unknown) => ({ body: null, error: typeof cause === 'string' ? cause : String(cause) }))
    ])
      .then(([loaded, bodyResult]) => {
        if (cancelled) return
        if (!loaded) {
          setDetail(null)
          setDetailError('未找到本地邮件记录。')
          return
        }
        setDetail(bodyResult?.body ? { ...loaded, body: bodyResult.body } : loaded)
        if (bodyResult?.error) setDetailError(bodyResult.error)
        else setDetailError(null)
      })
      .catch((cause: unknown) => {
        if (!cancelled) setDetailError(cause instanceof Error ? cause.message : String(cause))
      })
      .finally(() => {
        if (!cancelled) setDetailLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [selectedMessage])

  // 打开未读邮件时回写已读，并同步刷新左栏未读角标与中栏气泡样式
  React.useEffect(() => {
    if (!selectedMessage || selectedMessage.source !== 'mail' || selectedMessage.isRead) return
    const messageId = selectedMessage.messageId
    void window.api.messages
      .setReadState(messageId, true)
      .then(() => {
        setSelectedMessage((current) =>
          current && current.messageId === messageId ? { ...current, isRead: true } : current
        )
        // 后端已更新 onemail_correspondent_messages.is_read，这里重新取列表/角标/中栏
        void queryClient.invalidateQueries({ queryKey: ['correspondents'] })
        void queryClient.invalidateQueries({ queryKey: ['correspondent-thread'] })
      })
      .catch((cause: unknown) => {
        // Tauri 的 invoke 失败传递的是字符串，不是 Error，直接 instanceof 会吞掉真实原因
        toast.error(typeof cause === 'string' && cause.trim() ? cause : '标记已读失败')
      })
  }, [queryClient, selectedMessage])

  // 新建账号后：刷新账号列表与联系人索引（之前要手动「重新加载页面」才看得到数据）
  React.useEffect(() => {
    const stop = window.api.accounts.onCreated(() => {
      void queryClient.invalidateQueries({ queryKey: ['accounts'] })
      void queryClient.invalidateQueries({ queryKey: ['correspondents'] })
      toast.success('账号已添加，正在载入往来记录…')
    })
    return stop
  }, [queryClient])

  // 同步进度：底部状态栏显示「已完成/总数 + 当前文件夹」
  React.useEffect(() => {
    return window.api.sync.onProgress((event) => {
      setSyncProgress((current) => ({
        completed: event.completed ?? current?.completed ?? 0,
        total: event.total ?? current?.total ?? 0,
        folder: event.folder ?? current?.folder ?? null,
        accountId: event.accountId
      }))
    })
  }, [])

  // 同步完成后刷新
  React.useEffect(() => {
    return window.api.sync.onMailboxChanged(() => {
      void queryClient.invalidateQueries({ queryKey: ['correspondents'] })
    })
  }, [queryClient])

  /** 无边框窗口：顶栏空白处按住拖动，交互控件不触发。 */
  const handleTitlebarMouseDown = (event: React.MouseEvent<HTMLElement>): void => {
    if (event.button !== 0) return
    const target = event.target as HTMLElement
    if (target.closest('button, a, input, select, textarea')) return
    void getCurrentWindow().startDragging().catch(() => null)
  }

  /** 双击顶栏空白处：最大化 / 还原（与系统标题栏行为一致）。 */
  const handleTitlebarDoubleClick = (event: React.MouseEvent<HTMLElement>): void => {
    const target = event.target as HTMLElement
    if (target.closest('button, a, input, select, textarea')) return
    void getCurrentWindow().toggleMaximize().catch(() => null)
  }

  const syncAll = (): void => {
    setSyncing(true)
    void window.api.sync
      .startAll('refresh')
      .then(() => toast.success('已开始同步全部邮箱'))
      .catch((error: unknown) => toast.error(error instanceof Error ? error.message : '同步失败'))
      .finally(() => {
        setSyncing(false)
        setSyncProgress(null)
        void queryClient.invalidateQueries({ queryKey: ['correspondents'] })
      })
  }

  const startCompose = React.useCallback(
    async (kind: ComposeKind, related?: CorrespondentThreadMessage | null): Promise<void> => {
      const targetAccountId = related?.accountId ?? accounts[0]?.accountId ?? undefined
      if (!targetAccountId) {
        toast.error('还没有可用的发件账号，先添加一个邮箱。')
        openAddAccount()
        return
      }
      const relatedMessageId =
        related && related.source === 'mail' && kind !== 'new' ? related.messageId : undefined
      const effectiveKind: ComposeKind =
        kind !== 'new' && relatedMessageId === undefined ? 'new' : kind

      setComposePending(true)
      try {
        const draft = await createComposeDraft({
          kind: effectiveKind,
          accountId: targetAccountId,
          relatedMessageId
        })
        setComposeDraft(draft)
        setComposeOpen(true)
      } catch (error) {
        toast.error(error instanceof Error ? error.message : '创建草稿失败')
      } finally {
        setComposePending(false)
      }
    },
    // openAddAccount 是稳定闭包，不参与依赖
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [accounts]
  )

  const handleSend = React.useCallback(
    async (input: SendMessageInput): Promise<void> => {
      setComposePending(true)
      try {
        const result = await sendComposedMessage(input)
        if (result.warning) toast.warning(result.warning)
        else toast.success('邮件已发送')
        setComposeOpen(false)
        setComposeDraft(null)
        void queryClient.invalidateQueries({ queryKey: ['correspondents'] })
      } catch (error) {
        toast.error(error instanceof Error ? error.message : '发送失败')
      } finally {
        setComposePending(false)
      }
    },
    [queryClient]
  )

  const handleSaveDraft = React.useCallback(async (input: SendMessageInput): Promise<void> => {
    setComposePending(true)
    try {
      await saveComposedDraft(input)
      toast.success('已保存草稿')
      setComposeOpen(false)
      setComposeDraft(null)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '保存草稿失败')
    } finally {
      setComposePending(false)
    }
  }, [])

  const handleDiscardDraft = React.useCallback(async (draftId: number): Promise<void> => {
    try {
      await window.api.compose.deleteDraft(draftId)
    } catch {
      // 草稿可能尚未落库
    }
    setComposeOpen(false)
    setComposeDraft(null)
  }, [])

  const handleToggleJunk = React.useCallback(async (): Promise<void> => {
    if (!selectedMessage) return
    if (selectedMessage.source !== 'mail') {
      toast.error('发件箱记录没有垃圾属性。')
      return
    }
    const next = !selectedMessage.isJunk
    try {
      await window.api.messages.setJunkOverride(selectedMessage.messageId, next)
      toast.success(next ? '已标为垃圾邮件' : '已移回普通分组')
      setSelectedMessage((current) => (current ? { ...current, isJunk: next } : current))
      await queryClient.invalidateQueries({ queryKey: ['correspondents'] })
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '更新垃圾标记失败')
    }
  }, [queryClient, selectedMessage])

  const handleDelete = React.useCallback(async (): Promise<void> => {
    if (!selectedMessage) return
    if (selectedMessage.source !== 'mail') {
      toast.error('发件箱记录请在发件箱中删除。')
      return
    }
    try {
      await window.api.messages.setDeleted(selectedMessage.messageId, true)
      toast.success('已删除（仅本地）')
      setSelectedMessage(null)
      setDetail(null)
      await queryClient.invalidateQueries({ queryKey: ['correspondents'] })
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '删除失败')
    }
  }, [queryClient, selectedMessage])

  const startRename = (correspondent: CorrespondentSummary): void => {
    setRenameTarget(correspondent)
    setRenameValue(correspondent.alias ?? '')
  }

  const clearAlias = (correspondent: CorrespondentSummary): void => {
    void window.api.correspondents
      .setAlias(correspondent.correspondentId, null)
      .then(() => queryClient.invalidateQueries({ queryKey: ['correspondents'] }))
      .catch((error: unknown) => toast.error(error instanceof Error ? error.message : '清除备注名失败'))
  }

  const saveAlias = (): void => {
    if (!renameTarget) return
    setRenameSaving(true)
    void window.api.correspondents
      .setAlias(renameTarget.correspondentId, renameValue.trim() || null)
      .then(() => {
        toast.success('备注名已保存')
        setRenameTarget(null)
        return queryClient.invalidateQueries({ queryKey: ['correspondents'] })
      })
      .catch((error: unknown) => toast.error(error instanceof Error ? error.message : '保存备注名失败'))
      .finally(() => setRenameSaving(false))
  }

  const replyTarget = selectedMessage ?? thread?.messages[0] ?? null

  const accountOptions = React.useMemo(
    () =>
      accounts
        .filter((account): account is typeof account & { accountId: number } => Boolean(account.accountId))
        .map((account) => ({ accountId: account.accountId, label: account.name, badge: account.name.slice(0, 1) })),
    [accounts]
  )

  return (
    <div className="flex h-screen min-h-0 flex-col bg-background">
      {/* 顶栏：设计稿布局 —— 左侧留交通灯位、中间全局搜索、右侧同步/写邮件/菜单 */}
      <header
        onMouseDown={handleTitlebarMouseDown}
        onDoubleClick={handleTitlebarDoubleClick}
        className="app-titlebar app-drag-region flex h-12 shrink-0 items-center gap-3 border-b border-border pl-4 pr-3"
      >
        <div className="flex w-[220px] shrink-0 items-center gap-2.5">
          <span className="text-[13px] font-semibold">OneMail 往来</span>
        </div>

        <div className="app-no-drag relative mx-auto w-full max-w-[460px]" data-tauri-drag-region="false">
          <Search
            className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground"
            aria-hidden="true"
          />
          <input
            value={keyword}
            onChange={(event) => setKeyword(event.target.value)}
            placeholder="搜索联系人、邮箱地址或邮件主题"
            className="w-full rounded-lg border border-border bg-background py-1.5 pl-8 pr-12 text-[12px] outline-none focus:border-primary/60"
            aria-label="搜索联系人、邮箱地址或邮件主题"
          />
          {keyword ? (
            <button
              type="button"
              onClick={() => setKeyword('')}
              className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-0.5 text-muted-foreground hover:text-foreground"
              aria-label="清空搜索"
            >
              <X className="size-3.5" aria-hidden="true" />
            </button>
          ) : (
            <span className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-[10px] text-muted-foreground">
              ⌘K
            </span>
          )}
        </div>

        <div className="app-no-drag ml-auto flex shrink-0 items-center gap-1">
          <button
            type="button"
            onClick={syncAll}
            disabled={syncing || !hasAccounts}
            className="flex items-center gap-1 rounded-md px-2 py-1 text-[11.5px] text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-50"
          >
            <RefreshCw className={cn('size-3.5', syncing && 'animate-spin')} aria-hidden="true" />
            同步
          </button>
          <button
            type="button"
            onClick={() => void startCompose('new')}
            className="flex items-center gap-1 rounded-md bg-primary px-2 py-1 text-[11.5px] font-semibold text-primary-foreground hover:opacity-90"
          >
            <PenLine className="size-3.5" aria-hidden="true" />
            写邮件
          </button>
          <TopMenu onAddAccount={openAddAccount} onOpenSettings={openSettings} />
          <WindowControls />
        </div>
      </header>

      {!hasAccounts && !accountsQuery.isLoading ? (
        <main className="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 px-6 text-center">
          <span className="flex size-12 items-center justify-center rounded-2xl bg-primary/10 text-primary">
            <MailPlus className="size-6" aria-hidden="true" />
          </span>
          <h2 className="text-[15px] font-semibold">还没有添加邮箱账号</h2>
          <p className="max-w-md text-[11.5px] leading-relaxed text-muted-foreground">
            添加一个邮箱后，OneMail 会按来往联系人整理全部邮件。也可以先打开设置导入
            <code className="mx-1 rounded bg-muted px-1">.onemail</code> 备份。
          </p>
          <div className="mt-1 flex items-center gap-2">
            <button
              type="button"
              onClick={openAddAccount}
              className="flex items-center gap-1.5 rounded-lg bg-primary px-3 py-1.5 text-[12px] font-semibold text-primary-foreground hover:opacity-90"
            >
              <UserPlus className="size-3.5" aria-hidden="true" /> 添加邮箱账号
            </button>
            <button
              type="button"
              onClick={() => openSettings()}
              className="flex items-center gap-1.5 rounded-lg border border-border px-3 py-1.5 text-[12px] hover:bg-muted"
            >
              <Settings className="size-3.5" aria-hidden="true" /> 打开设置
            </button>
          </div>
        </main>
      ) : (
        <div className="flex min-h-0 flex-1">
          <CorrespondentList
            group={group}
            onGroupChange={(next) => {
              setGroup(next)
              setSelectedId(null)
            }}
            counts={counts}
            items={list.items}
            selectedId={selectedId}
            onSelect={(correspondentId) => {
              setSelectedId(correspondentId)
              setComposeOpen(false)
              setComposeDraft(null)
            }}
            loading={list.loading}
            nextCursor={list.nextCursor}
            loadMore={list.loadMore}
            accounts={accountOptions}
            accountId={accountId}
            onAccountChange={(next) => {
              setAccountId(next)
              setSelectedId(null)
            }}
            onRename={startRename}
            onClearAlias={clearAlias}
            searching={keyword.trim().length > 0}
          />

          <CorrespondentThread
            thread={thread ?? undefined}
            loading={threadLoading}
            selectedKey={selectedMessage ? messageKey(selectedMessage) : null}
            onSelectMessage={setSelectedMessage}
            onReplyTo={(message) => void startCompose('reply', message)}
            onReply={() => void startCompose(replyTarget ? 'reply' : 'new', replyTarget)}
            onCompose={() => void startCompose('new', null)}
          />

          {composeOpen ? (
            <section className="flex min-h-0 flex-1 flex-col bg-background">
              <MailComposer
                open={composeOpen}
                variant="inline"
                accounts={accounts}
                draft={composeDraft}
                pending={composePending}
                onOpenChange={(open) => {
                  if (!open) {
                    setComposeOpen(false)
                    setComposeDraft(null)
                  }
                }}
                onSend={handleSend}
                onSaveDraft={handleSaveDraft}
                onDiscardDraft={handleDiscardDraft}
              />
            </section>
          ) : (
            <CorrespondentMessageReader
              message={selectedMessage}
              contactAddress={thread?.correspondent.address}
              detail={detail}
              outbox={outboxDetail}
              loading={detailLoading}
              error={detailError}
              onReply={() => void startCompose('reply', replyTarget)}
              onReplyAll={() => void startCompose('reply_all', replyTarget)}
              onForward={() => void startCompose('forward', replyTarget)}
              onToggleJunk={() => void handleToggleJunk()}
              onDelete={() => void handleDelete()}
            />
          )}
        </div>
      )}

      {renameTarget ? (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
          <div className="w-[320px] rounded-xl border border-border bg-background p-4 shadow-xl">
            <p className="text-[13px] font-semibold">设置备注名</p>
            <p className="mt-1 truncate text-[11px] text-muted-foreground">{renameTarget.address}</p>
            <input
              autoFocus
              value={renameValue}
              onChange={(event) => setRenameValue(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') saveAlias()
                if (event.key === 'Escape') setRenameTarget(null)
              }}
              placeholder="例如：王经理（留空则恢复原名）"
              className="mt-3 w-full rounded-lg border border-border bg-background px-2.5 py-1.5 text-[12px] outline-none focus:border-primary/60"
            />
            <div className="mt-3 flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setRenameTarget(null)}
                className="rounded-md border border-border px-2.5 py-1 text-[11.5px] hover:bg-muted"
              >
                取消
              </button>
              <button
                type="button"
                disabled={renameSaving}
                onClick={saveAlias}
                className="rounded-md bg-primary px-2.5 py-1 text-[11.5px] font-semibold text-primary-foreground hover:opacity-90 disabled:opacity-50"
              >
                保存
              </button>
            </div>
          </div>
        </div>
      ) : null}

      <SettingsDialogHost open={settingsOpen} onClose={() => setSettingsOpen(false)} />

      <footer className="flex h-7 shrink-0 items-center gap-3 border-t border-border bg-[var(--native-statusbar)] px-3 text-[10.5px] text-muted-foreground">
        {syncProgress ? (
          <span className="flex items-center gap-1.5 text-primary">
            <RefreshCw className="size-3 animate-spin" aria-hidden="true" />
            同步中 {syncProgress.completed}/{syncProgress.total || '?'}
            {syncProgress.folder ? ` · ${syncProgress.folder}` : ''}
          </span>
        ) : null}
        <span>{counts ? `${counts.normal.contacts} 位联系人` : '—'}</span>
        <span>普通未读 {counts?.normal.unread ?? 0}</span>
        <span>垃圾未读 {counts?.junk.unread ?? 0}</span>
        <div className="flex-1" />
        <span>{accounts.length} 个邮箱 · 写信内嵌 · 删除仅本地</span>
      </footer>
    </div>
  )
}
