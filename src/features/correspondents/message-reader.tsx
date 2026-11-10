import * as React from 'react'
import { CornerUpLeft, CornerUpRight, Forward, Mail, ShieldAlert, Trash2 } from 'lucide-react'

import { formatAbsoluteTime } from '@renderer/components/mail/date-format'
import type { CorrespondentThreadMessage, MailMessageDetail } from '@renderer/shared/types'

/**
 * 右栏正文（阶段 2 轻量版）：先用 messages.get + loadBody 渲染，
 * 后续再替换为完整的 ConversationPane（含回复/转发/归档与附件表）。
 */
export function CorrespondentMessageReader({
  message,
  detail,
  loading,
  error,
  onReply,
  onReplyAll,
  onForward,
  onDelete,
  onToggleJunk
}: {
  message: CorrespondentThreadMessage | null
  detail: MailMessageDetail | null
  loading: boolean
  error: string | null
  onReply: () => void
  onReplyAll: () => void
  onForward: () => void
  onDelete: () => void
  /** 当前邮件是否被判为垃圾，用来决定按钮是「不是垃圾」还是「标为垃圾」 */
  onToggleJunk: () => void
}): React.JSX.Element {
  if (!message) {
    return (
      <section className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 bg-background text-muted-foreground">
        <Mail className="size-8 opacity-40" aria-hidden="true" />
        <p className="text-[11.5px]">点击中间任意一条标题查看正文</p>
      </section>
    )
  }

  const subject = detail?.subject ?? message.subject ?? '(无主题)'
  const from = [detail?.fromName, detail?.fromEmail].filter(Boolean).join(' · ')
  const bodyText = detail?.body?.bodyText ?? ''
  const bodyHtml = detail?.body?.bodyHtmlSanitized ?? ''

  return (
    <section className="flex min-h-0 flex-1 flex-col bg-background">
      <header className="flex h-11 shrink-0 items-center gap-3 border-b border-border px-3 text-[12px]">
        <button
          type="button"
          onClick={onReply}
          className="flex items-center gap-1 font-semibold hover:text-primary"
        >
          <CornerUpLeft className="size-3.5" aria-hidden="true" /> 回复
        </button>
        <button
          type="button"
          onClick={onReplyAll}
          className="flex items-center gap-1 text-muted-foreground hover:text-foreground"
        >
          <CornerUpRight className="size-3.5" aria-hidden="true" /> 回复全部
        </button>
        <button
          type="button"
          onClick={onForward}
          className="flex items-center gap-1 text-muted-foreground hover:text-foreground"
        >
          <Forward className="size-3.5" aria-hidden="true" /> 转发
        </button>
        <button
          type="button"
          onClick={onToggleJunk}
          className="flex items-center gap-1 text-muted-foreground hover:text-foreground"
          title={message.isJunk ? '这封不是垃圾邮件，移回普通分组' : '把这一封标记为垃圾邮件'}
        >
          <ShieldAlert className="size-3.5" aria-hidden="true" />
          {message.isJunk ? '不是垃圾' : '标为垃圾'}
        </button>
        <button
          type="button"
          onClick={onDelete}
          className="flex items-center gap-1 text-muted-foreground hover:text-destructive"
        >
          <Trash2 className="size-3.5" aria-hidden="true" /> 删除
        </button>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
        <h1 className="text-[16.5px] font-bold leading-snug">{subject}</h1>
        {message.isJunk ? (
          <p className="mt-1 inline-flex items-center gap-1 rounded-full bg-[#f2f2f5] px-2 py-0.5 text-[10px] text-[#9a9aa4]">
            <ShieldAlert className="size-3" aria-hidden="true" /> 垃圾邮件 · 已移入垃圾箱
          </p>
        ) : null}

        <div className="mt-3 flex items-start justify-between gap-4 border-b border-border pb-3 text-[11px]">
          <div className="min-w-0">
            <p className="truncate font-semibold">{from || '—'}</p>
            <p className="truncate text-muted-foreground">{detail?.to ? `收件人 ${detail.to}` : ''}</p>
            {detail?.cc ? <p className="truncate text-muted-foreground">抄送 {detail.cc}</p> : null}
          </div>
          <span className="shrink-0 text-muted-foreground">{formatAbsoluteTime(message.sentAt)}</span>
        </div>

        {message.isJunk ? (
          <div className="mt-3 rounded-lg border border-[#f2e0a8] bg-[#fff8e1] px-3 py-1.5 text-[11px] text-[#8a6d1f]">
            为保护隐私，已阻止远程内容
          </div>
        ) : null}

        {loading ? <p className="mt-4 text-[11.5px] text-muted-foreground">正在加载正文…</p> : null}
        {error ? <p className="mt-4 text-[11.5px] text-destructive">{error}</p> : null}

        {!loading && !error ? (
          bodyHtml ? (
            // 后端已完成净化（body_html_sanitized），远程图片默认被阻止
            <div
              className="mt-4 text-[12.5px] leading-relaxed [&_a]:text-primary [&_img]:max-w-full"
              dangerouslySetInnerHTML={{ __html: bodyHtml }}
            />
          ) : (
            <pre className="mt-4 whitespace-pre-wrap break-words font-sans text-[12.5px] leading-relaxed">
              {bodyText || '（正文尚未缓存，点开时按需加载）'}
            </pre>
          )
        ) : null}

        {detail?.attachments?.length ? (
          <div className="mt-5 border-t border-border pt-3">
            <p className="text-[11px] font-semibold">附件（元数据）</p>
            <ul className="mt-1 space-y-1 text-[11px] text-muted-foreground">
              {detail.attachments.map((attachment) => (
                <li key={attachment.attachmentId} className="truncate">
                  {attachment.filename} · {attachment.mimeType ?? '未知类型'} ·{' '}
                  {Math.max(1, Math.round((attachment.sizeBytes ?? 0) / 1024))} KB
                </li>
              ))}
            </ul>
          </div>
        ) : null}
      </div>
    </section>
  )
}
