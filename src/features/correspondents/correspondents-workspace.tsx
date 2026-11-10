import * as React from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { PenLine, RefreshCw } from 'lucide-react'
import { toast } from 'sonner'

import { MailComposer } from '@renderer/components/mail/mail-composer'
import { createComposeDraft, loadAccounts, saveComposedDraft, sendComposedMessage } from '@renderer/lib/api'
import type { ComposeDraft, ComposeKind, SendMessageInput } from '@renderer/lib/api/types'
import type { CorrespondentGroup, CorrespondentThreadMessage, MailMessageDetail } from '@renderer/shared/types'

import { CorrespondentList } from './correspondent-list'
import { CorrespondentThread } from './correspondent-thread'
import { CorrespondentMessageReader } from './message-reader'
import { useCorrespondentGroupCounts, useCorrespondentList, useCorrespondentThread } from './use-correspondents'

function messageKey(message: CorrespondentThreadMessage): string {
  return `${message.source}:${message.messageId}`
}

export function CorrespondentsWorkspace(): React.JSX.Element {
  const queryClient = useQueryClient()
  const [group, setGroup] = React.useState<CorrespondentGroup>('normal')
  const [keyword, setKeyword] = React.useState('')
  const deferredKeyword = React.useDeferredValue(keyword)
  const [selectedId, setSelectedId] = React.useState<number | null>(null)
  const [selectedMessage, setSelectedMessage] = React.useState<CorrespondentThreadMessage | null>(null)
  const [detail, setDetail] = React.useState<MailMessageDetail | null>(null)
  const [detailLoading, setDetailLoading] = React.useState(false)
  const [detailError, setDetailError] = React.useState<string | null>(null)
  const [refreshing, setRefreshing] = React.useState(false)

  // 写信状态（内嵌右栏）
  const [composeOpen, setComposeOpen] = React.useState(false)
  const [composeDraft, setComposeDraft] = React.useState<ComposeDraft | null>(null)
  const [composePending, setComposePending] = React.useState(false)

  const { counts } = useCorrespondentGroupCounts()
  const list = useCorrespondentList(group, deferredKeyword)
  const { thread, loading: threadLoading } = useCorrespondentThread(selectedId)
  const accountsQuery = useQuery({ queryKey: ['accounts'], queryFn: loadAccounts, staleTime: 60_000 })
  const accounts = accountsQuery.data ?? []

  // 选中联系人变化时清空右侧
  React.useEffect(() => {
    setSelectedMessage(null)
    setDetail(null)
    setDetailError(null)
  }, [selectedId])

  // 正文按需加载
  React.useEffect(() => {
    if (!selectedMessage) return
    if (selectedMessage.source !== 'mail') {
      setDetail(null)
      setDetailError('该记录来自发件箱，正文请在发件箱查看。')
      return
    }
    let cancelled = false
    setDetailLoading(true)
    setDetailError(null)
    const messageId = selectedMessage.messageId
    Promise.resolve(window.api.messages.loadBody(messageId))
      .catch(() => null)
      .then(() => window.api.messages.get(messageId))
      .then((loaded) => {
        if (cancelled) return
        setDetail(loaded)
        if (!loaded) setDetailError('未找到本地邮件记录。')
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

  // 打开未读邮件时回写已读（会触发索引重算与角标刷新）
  React.useEffect(() => {
    if (!selectedMessage || selectedMessage.source !== 'mail' || selectedMessage.isRead) return
    void window.api.messages.setReadState(selectedMessage.messageId, true).catch(() => null)
  }, [selectedMessage])

  // 同步完成后刷新联系人分组与列表
  React.useEffect(() => {
    return window.api.sync.onMailboxChanged(() => {
      void queryClient.invalidateQueries({ queryKey: ['correspondents'] })
    })
  }, [queryClient])

  const refresh = (): void => {
    setRefreshing(true)
    void queryClient
      .invalidateQueries({ queryKey: ['correspondents'] })
      .finally(() => setRefreshing(false))
  }

  /** 打开内嵌写信：回复/回复全部需要一条 mail 来源的邮件，新邮件不需要。 */
  const startCompose = React.useCallback(
    async (kind: ComposeKind, related?: CorrespondentThreadMessage | null): Promise<void> => {
      const accountId =
        related?.accountId ?? accounts[0]?.accountId ?? undefined
      if (!accountId) {
        toast.error('还没有可用的发件账号，请先在设置里添加邮箱。')
        return
      }
      // 回复/回复全部/转发都需要一条 mail 来源的邮件；发件箱记录只能降级为新邮件
      const relatedMessageId =
        related && related.source === 'mail' && kind !== 'new' ? related.messageId : undefined
      const effectiveKind: ComposeKind =
        kind !== 'new' && relatedMessageId === undefined ? 'new' : kind

      setComposePending(true)
      try {
        const draft = await createComposeDraft({
          kind: effectiveKind,
          accountId,
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

  const handleSaveDraft = React.useCallback(
    async (input: SendMessageInput): Promise<void> => {
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
    },
    []
  )

  const handleDiscardDraft = React.useCallback(async (draftId: number): Promise<void> => {
    try {
      await window.api.compose.deleteDraft(draftId)
    } catch {
      // 草稿可能尚未落库，忽略
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

  const replyTarget = selectedMessage ?? thread?.messages[0] ?? null

  return (
    <div className="flex h-screen min-h-0 flex-col bg-background">
      <header className="app-titlebar app-drag-region flex h-11 shrink-0 items-center gap-3 border-b border-border px-[var(--app-titlebar-left-offset)] pr-3">
        <span className="text-[13px] font-semibold">OneMail 往来</span>
        <span className="text-[10.5px] text-muted-foreground">按人归并的往来时间线</span>
        <div className="flex-1" />
        <button
          type="button"
          onClick={refresh}
          className="flex items-center gap-1 text-[11px] text-muted-foreground hover:text-foreground"
        >
          <RefreshCw className={refreshing ? 'size-3.5 animate-spin' : 'size-3.5'} aria-hidden="true" />
          刷新
        </button>
        <button
          type="button"
          onClick={() => void startCompose('new')}
          className="flex items-center gap-1 rounded-md bg-primary px-2 py-1 text-[11.5px] font-semibold text-primary-foreground hover:opacity-90"
        >
          <PenLine className="size-3.5" aria-hidden="true" />
          写邮件
        </button>
      </header>

      <div className="flex min-h-0 flex-1">
        <CorrespondentList
          group={group}
          onGroupChange={(next) => {
            setGroup(next)
            setSelectedId(null)
          }}
          counts={counts}
          keyword={keyword}
          onKeywordChange={setKeyword}
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
        />

        <CorrespondentThread
          thread={thread ?? undefined}
          loading={threadLoading}
          selectedKey={selectedMessage ? messageKey(selectedMessage) : null}
          onSelectMessage={setSelectedMessage}
          onReply={() => void startCompose(replyTarget ? 'reply' : 'new', replyTarget)}
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
            detail={detail}
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

      <footer className="flex h-7 shrink-0 items-center gap-3 border-t border-border bg-[var(--native-statusbar)] px-3 text-[10.5px] text-muted-foreground">
        <span>{counts ? `${counts.normal.contacts} 位联系人` : '—'}</span>
        <span>普通未读 {counts?.normal.unread ?? 0}</span>
        <span>垃圾未读 {counts?.junk.unread ?? 0}</span>
        <div className="flex-1" />
        <span>写信内嵌 · 垃圾归属可逐封纠正 · 删除仅本地</span>
      </footer>
    </div>
  )
}
