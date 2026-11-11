import * as React from 'react'
import { Paperclip, Plus, Reply } from 'lucide-react'

import { ContextMenu, useContextMenu } from '@renderer/components/context-menu'
import { formatRelativeTime } from '@renderer/components/mail/date-format'
import { subjectKey, useTranslation } from './use-translation'
import { cn } from '@renderer/lib/utils'
import type {
  CorrespondentProfile,
  CorrespondentThreadMessage,
  CorrespondentThreadResult
} from '@renderer/shared/types'

import { groupThreadByDay, threadMessageKey } from './correspondent-utils'

function Header({
  profile,
  onReply
}: {
  profile: CorrespondentProfile
  onReply: () => void
}): React.JSX.Element {
  return (
    <header className="flex h-14 shrink-0 items-center gap-2 border-b border-border px-3">
      <span
        className="flex size-8 shrink-0 items-center justify-center rounded-full text-[13px] font-semibold text-white"
        style={{ backgroundColor: profile.accounts[0]?.color ?? '#2d70d2' }}
        aria-hidden="true"
      >
        {(profile.displayName?.trim() || profile.address).slice(0, 1).toUpperCase()}
      </span>
      <div className="min-w-0 flex-1">
        <p className="truncate text-[13px] font-semibold">
          {profile.alias?.trim() || profile.displayName?.trim() || profile.address}
        </p>
        <p className="truncate text-[10.5px] text-muted-foreground">
          {profile.address} · 往来 {profile.inboundCount + profile.outboundCount} 封
        </p>
      </div>
      <div className="flex items-center gap-1">
        {profile.accounts.slice(0, 2).map((account) => (
          <span
            key={account.accountId}
            className="rounded-[4px] px-1 text-[9px] font-bold leading-4 text-white"
            style={{ backgroundColor: account.color ?? '#2d70d2' }}
            title={`${account.badge} · ${account.email}`}
          >
            {account.badge}
          </span>
        ))}
        {profile.accounts.length > 2 ? (
          <span
            className="rounded-[4px] bg-muted px-1 text-[9px] font-semibold leading-4 text-muted-foreground"
            title={profile.accounts.map((account) => `${account.badge} · ${account.email}`).join('\n')}
          >
            +{profile.accounts.length - 2}
          </span>
        ) : null}
      </div>
      <button
        type="button"
        onClick={onReply}
        className="flex size-7 items-center justify-center rounded-md border border-border text-muted-foreground hover:bg-muted"
        title="新话题"
      >
        <Plus className="size-3.5" aria-hidden="true" />
      </button>
    </header>
  )
}

function Bubble({
  message,
  selected,
  onSelect,
  onReply
}: {
  message: CorrespondentThreadMessage
  selected: boolean
  onSelect: () => void
  onReply: () => void
}): React.JSX.Element {
  const { menu, open, close } = useContextMenu()
  const translation = useTranslation()
  const translationKey = subjectKey(message.source, message.messageId)
  const translatedSubject = translation.translationOf(translationKey)
  const translating = translation.pendingKey === translationKey
  const mine = message.direction === 'out'
  const subject = message.subject?.trim() || '(无主题)'
  // 只有收到的邮件才有未读概念；发件箱记录一律按已读展示
  const unread = !mine && !message.isRead
  return (
    <div className={cn('flex flex-col', mine ? 'items-end' : 'items-start')}>
      <button
        type="button"
        onClick={onSelect}
        onContextMenu={(event) =>
          open(event, [
            { label: '回复', onSelect: onReply },
            {
              label: translating ? '翻译中…' : translatedSubject ? '收起译文' : '翻译标题',
              disabled: translating,
              onSelect: () => void translation.translate(translationKey, subject)
            }
          ])
        }
        className={cn(
          'flex max-w-[86%] items-center gap-1.5 rounded-[11px] px-3 py-1.5 text-left text-[12px] leading-snug',
          mine
            ? 'bg-primary text-primary-foreground'
            : message.isJunk
              ? 'border border-dashed border-border bg-muted/60 text-[#4a4a52] dark:text-muted-foreground'
              : unread
                ? 'border border-primary/30 bg-background hover:bg-muted/40'
                : 'border border-border bg-background hover:bg-muted/40',
          selected && !mine ? 'ring-2 ring-primary/40' : '',
          selected && mine ? 'ring-2 ring-primary/60 ring-offset-1' : ''
        )}
        title={subject}
      >
        <span className={cn('truncate', unread && 'font-semibold')}>{subject}</span>
        {message.hasAttachment ? <Paperclip className="size-3 shrink-0 opacity-70" aria-hidden="true" /> : null}
        {unread ? (
          <span
            className="size-1.5 shrink-0 rounded-full bg-primary"
            title="未读"
            aria-label="未读"
          />
        ) : null}
        {message.isJunk ? (
          <span className="shrink-0 rounded-full bg-[#f2f2f5] px-1.5 text-[9px] font-semibold text-[#9a9aa4]">
            垃圾
          </span>
        ) : null}
      </button>
      <span className="mt-0.5 px-1 text-[9.5px] text-muted-foreground">
        {formatRelativeTime(message.sentAt)}
        {message.isJunk ? ' · 垃圾邮件' : ''}
        {unread ? ' · 未读' : ''}
      </span>
      {translatedSubject ? (
        <span className={cn('mt-0.5 max-w-[86%] px-1 text-[10.5px] text-primary/80', mine && 'text-right')}>
          {translatedSubject}
        </span>
      ) : null}
      <ContextMenu menu={menu} onClose={close} />
    </div>
  )
}

export function CorrespondentThread({
  thread,
  loading,
  selectedKey,
  onSelectMessage,
  onReplyTo,
  onReply
}: {
  thread: CorrespondentThreadResult | undefined
  loading: boolean
  selectedKey: string | null
  onSelectMessage: (message: CorrespondentThreadMessage) => void
  /** 右键某一封气泡时的「回复」：以该封为引用对象 */
  onReplyTo: (message: CorrespondentThreadMessage) => void
  onReply: () => void
}): React.JSX.Element {
  const grouped = React.useMemo(() => groupThreadByDay(thread?.messages ?? []), [thread])

  if (!thread) {
    return (
      <section className="flex min-h-0 w-[380px] shrink-0 flex-col items-center justify-center border-r border-border bg-background text-[11.5px] text-muted-foreground">
        {loading ? '正在载入往来…' : '从左侧选择一个联系人'}
      </section>
    )
  }

  return (
    <section className="flex min-h-0 w-[380px] shrink-0 flex-col border-r border-border bg-background">
      <Header profile={thread.correspondent} onReply={onReply} />

      <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto px-3 py-3">
        {grouped.map((bucket) => (
          <React.Fragment key={bucket.day}>
            <div className="my-1 flex items-center gap-2">
              <span className="h-px flex-1 bg-border" aria-hidden="true" />
              <span className="rounded-full bg-muted px-2 py-0.5 text-[9.5px] text-muted-foreground">
                {bucket.day}
              </span>
              <span className="h-px flex-1 bg-border" aria-hidden="true" />
            </div>
            {bucket.items.map((message) => (
              <Bubble
                key={threadMessageKey(message)}
                message={message}
                selected={selectedKey === threadMessageKey(message)}
                onSelect={() => onSelectMessage(message)}
                onReply={() => onReplyTo(message)}
              />
            ))}
          </React.Fragment>
        ))}
        {grouped.length === 0 ? (
          <p className="py-6 text-center text-[11px] text-muted-foreground">还没有往来记录</p>
        ) : null}
      </div>

      <footer className="shrink-0 border-t border-border bg-[var(--native-toolbar)] p-2">
        <button
          type="button"
          onClick={onReply}
          className="flex w-full items-center justify-center gap-1.5 rounded-[10px] bg-primary py-2.5 text-[12.5px] font-semibold text-primary-foreground hover:opacity-90"
        >
          <Reply className="size-3.5" aria-hidden="true" />
          回复 {thread.correspondent.displayName?.trim() || thread.correspondent.address}
        </button>
      </footer>
    </section>
  )
}
