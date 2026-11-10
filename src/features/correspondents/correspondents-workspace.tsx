import * as React from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { PenLine, RefreshCw } from 'lucide-react'

import type { CorrespondentGroup, CorrespondentThreadMessage, MailMessageDetail } from '@renderer/shared/types'

import { CorrespondentList } from './correspondent-list'
import { CorrespondentThread } from './correspondent-thread'
import { CorrespondentMessageReader } from './message-reader'
import { useCorrespondentGroupCounts, useCorrespondentList, useCorrespondentThread } from './use-correspondents'

function messageKey(message: CorrespondentThreadMessage): string {
  return `${message.source}:${message.messageId}`
}

/** 阶段性占位：下一步把 MailComposer 改成 pane 内嵌 variant 后替换。 */
function ComposePlaceholder({ onClose }: { onClose: () => void }): React.JSX.Element {
  return (
    <section className="flex min-h-0 flex-1 flex-col bg-background">
      <header className="flex h-10 shrink-0 items-center justify-between border-b border-border px-3">
        <span className="text-[12px] font-semibold">写信</span>
        <button type="button" onClick={onClose} className="text-[11px] text-muted-foreground hover:text-foreground">
          关闭
        </button>
      </header>
      <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 text-muted-foreground">
        <PenLine className="size-8 opacity-40" aria-hidden="true" />
        <p className="text-[11.5px]">写信界面将在此内嵌打开（下一步接入 MailComposer）</p>
      </div>
    </section>
  )
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
  const [composeOpen, setComposeOpen] = React.useState(false)
  const [refreshing, setRefreshing] = React.useState(false)

  const { counts } = useCorrespondentGroupCounts()
  const list = useCorrespondentList(group, deferredKeyword)
  const { thread, loading: threadLoading } = useCorrespondentThread(selectedId)

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

  // 打开未读邮件时回写已读
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
          onClick={() => setComposeOpen(true)}
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
          onReply={() => setComposeOpen(true)}
        />

        {composeOpen ? (
          <ComposePlaceholder onClose={() => setComposeOpen(false)} />
        ) : (
          <CorrespondentMessageReader
            message={selectedMessage}
            detail={detail}
            loading={detailLoading}
            error={detailError}
          />
        )}
      </div>

      <footer className="flex h-7 shrink-0 items-center gap-3 border-t border-border bg-[var(--native-statusbar)] px-3 text-[10.5px] text-muted-foreground">
        <span>{counts ? `${counts.normal.contacts} 位联系人` : '—'}</span>
        <span>普通未读 {counts?.normal.unread ?? 0}</span>
        <span>垃圾未读 {counts?.junk.unread ?? 0}</span>
        <div className="flex-1" />
        <span>阶段 2：左栏 + 中栏已接入索引，写信待接入</span>
      </footer>
    </div>
  )
}
