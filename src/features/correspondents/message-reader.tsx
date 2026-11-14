import * as React from 'react'
import { CornerUpLeft, CornerUpRight, Forward, Languages, Mail, ShieldAlert, Trash2 } from 'lucide-react'

import { formatAbsoluteTime } from '@renderer/components/mail/date-format'
import { toast } from 'sonner'

import { cn } from '@renderer/lib/utils'
import type { CorrespondentThreadMessage, MailMessageDetail, OutboxMessage } from '@renderer/shared/types'
import { bodyKey, splitParagraphs, useParagraphTranslation, useTranslation } from './use-translation'

/**
 * 右栏正文（阶段 2 轻量版）：先用 messages.get + loadBody 渲染，
 * 后续再替换为完整的 ConversationPane（含回复/转发/归档与附件表）。
 */
export function CorrespondentMessageReader({
  message,
  detail,
  outbox,
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
  /** 记录来自发件箱时，直接渲染这封已发邮件的完整内容 */
  outbox: OutboxMessage | null
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

  const isOutbox = message.source === 'outbox'
  const subject = outbox?.subject ?? detail?.subject ?? message.subject ?? '(无主题)'
  const from = isOutbox
    ? '我（已发送）'
    : [detail?.fromName, detail?.fromEmail].filter(Boolean).join(' · ')
  const toLine = isOutbox
    ? (outbox?.to ?? []).map((item) => item.email).filter(Boolean).join(', ')
    : (detail?.to ?? '')
  const ccLine = isOutbox
    ? (outbox?.cc ?? []).map((item) => item.email).filter(Boolean).join(', ')
    : (detail?.cc ?? '')
  const bodyText = isOutbox ? (outbox?.bodyText ?? '') : (detail?.body?.bodyText ?? '')
  const bodyHtml = isOutbox ? (outbox?.bodyHtml ?? '') : (detail?.body?.bodyHtmlSanitized ?? '')

  const translation = useTranslation()
  const paragraphTranslation = useParagraphTranslation()
  const paragraphs = React.useMemo(() => (bodyText ? splitParagraphs(bodyText) : []), [bodyText])
  const paragraphsNeeded = paragraphs.length > 0 && paragraphs.length <= 40
  const translatedParagraphs = message ? paragraphTranslation.paragraphsOf(message.messageId) : undefined
  const [attachmentBusy, setAttachmentBusy] = React.useState<string | null>(null)
  const [attachments, setAttachments] = React.useState<
    { index: number; filename: string; mimeType: string; sizeBytes: number }[]
  >([])
  const [preview, setPreview] = React.useState<
    { index: number; filename: string; mimeType: string; dataUrl: string } | null
  >(null)

  /**
   * 附件元数据来自 messages_get；但老库/同步阶段可能没写全，
   * 这里按需补齐：优先用 detail.attachments，其次沿用之前列表。
   */
  React.useEffect(() => {
    const list = (detail?.attachments ?? []).map((item, index) => ({
      index,
      filename: item.filename,
      mimeType: item.mimeType ?? '',
      sizeBytes: item.sizeBytes ?? 0
    }))
    setAttachments(list)
    setPreview(null)
  }, [detail])

  function busyKey(action: string, index: number): string {
    return `${action}:${index}`
  }

  /** 预览：图片直接内嵌显示；其它类型交给系统程序打开。 */
  async function previewAttachment(index: number): Promise<void> {
    if (!message) return
    setAttachmentBusy(busyKey('preview', index))
    try {
      const payload = await window.api.messages.attachmentPayload(message.messageId, index)
      const isImage = payload.mimeType.startsWith('image/') || /\.(png|jpe?g|gif|webp|bmp)$/i.test(payload.filename)
      if (isImage && payload.sizeBytes <= 8 * 1024 * 1024) {
        setPreview({
          index,
          filename: payload.filename,
          mimeType: payload.mimeType,
          dataUrl: `data:${payload.mimeType || 'image/png'};base64,${payload.base64}`
        })
      } else {
        // PDF 等 WebView 内无插件可渲染：落盘后用系统程序打开
        const path = await window.api.messages.saveAttachment(message.messageId, index)
        await window.api.system.openPath(path)
        toast.message('该类型不支持内嵌预览，已用系统程序打开')
      }
    } catch (cause: unknown) {
      toast.error(typeof cause === 'string' ? cause : '预览附件失败')
    } finally {
      setAttachmentBusy(null)
    }
  }

  /** 下载：保存到「下载」目录，重名自动加序号。 */
  async function downloadAttachment(index: number): Promise<void> {
    if (!message) return
    setAttachmentBusy(busyKey('download', index))
    try {
      const path = await window.api.messages.downloadAttachment(message.messageId, index)
      toast.success(`已保存到 ${path}`)
    } catch (cause: unknown) {
      toast.error(typeof cause === 'string' ? cause : '下载附件失败')
    } finally {
      setAttachmentBusy(null)
    }
  }
  const translationKey = bodyKey(message.messageId)
  const translatedBody = translation.translationOf(translationKey)
  const translating = translation.pendingKey === translationKey

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
          onClick={() => {
            if (!message) return
            if (translatedParagraphs) {
              paragraphTranslation.clear(message.messageId)
            } else if (paragraphsNeeded) {
              void paragraphTranslation.translateParagraphs(message.messageId, paragraphs)
            } else {
              void translation.translate(translationKey, bodyText)
            }
          }}
          disabled={!bodyText || translating || paragraphTranslation.pending}
          title={translatedParagraphs ? '收起译文' : '翻译正文'}
          className={cn(
            'flex items-center gap-1 disabled:opacity-40',
            translatedBody ? 'font-semibold text-primary' : 'text-muted-foreground hover:text-foreground'
          )}
        >
          <Languages
            className={cn('size-3.5', (translating || paragraphTranslation.pending) && 'animate-pulse')}
            aria-hidden="true"
          />
          {translating || paragraphTranslation.pending
            ? '翻译中…'
            : translatedParagraphs || translatedBody
              ? '收起译文'
              : '翻译'}
        </button>
        <button
          type="button"
          onClick={onForward}
          className="flex items-center gap-1 text-muted-foreground hover:text-foreground"
        >
          <Forward className="size-3.5" aria-hidden="true" /> 转发
        </button>
        {isOutbox ? null : (
        <button
          type="button"
          onClick={onToggleJunk}
          className="flex items-center gap-1 text-muted-foreground hover:text-foreground"
          title={message.isJunk ? '这封不是垃圾邮件，移回普通分组' : '把这一封标记为垃圾邮件'}
        >
          <ShieldAlert className="size-3.5" aria-hidden="true" />
          {message.isJunk ? '不是垃圾' : '标为垃圾'}
        </button>
        )}
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
            <p className="truncate text-muted-foreground">{toLine ? `收件人 ${toLine}` : ''}</p>
            {ccLine ? <p className="truncate text-muted-foreground">抄送 {ccLine}</p> : null}
          </div>
          <span className="shrink-0 text-muted-foreground">{formatAbsoluteTime(outbox?.sentAt ?? message.sentAt)}</span>
        </div>

        {message.isJunk ? (
          <div className="mt-3 rounded-lg border border-[#f2e0a8] bg-[#fff8e1] px-3 py-1.5 text-[11px] text-[#8a6d1f]">
            为保护隐私，已阻止远程内容
          </div>
        ) : null}

        {loading ? <p className="mt-4 text-[11.5px] text-muted-foreground">正在加载正文…</p> : null}
        {error ? <p className="mt-4 text-[11.5px] text-destructive">{error}</p> : null}

        {!loading && !error ? (
          <>
            {translatedParagraphs ? (
              // 沉浸式风格：每段原文下面紧跟该段译文
              <div className="mt-4 space-y-3 text-[12.5px] leading-relaxed">
                {paragraphs.map((paragraph, index) => (
                  <div key={index}>
                    <p className="whitespace-pre-wrap break-words">{paragraph}</p>
                    {translatedParagraphs[index] ? (
                      <p className="mt-1 whitespace-pre-wrap break-words border-l-2 border-primary/40 pl-2 text-[12.5px] text-primary/90">
                        {translatedParagraphs[index]}
                      </p>
                    ) : null}
                  </div>
                ))}
              </div>
            ) : translatedBody ? (
              <div className="mt-4 mb-3 rounded-lg border border-primary/25 bg-primary/5 p-3 text-[13px] leading-relaxed whitespace-pre-wrap">
                <p className="mb-1 text-[10.5px] font-semibold text-primary">译文</p>
                {translatedBody}
              </div>
            ) : null}
            {translatedParagraphs ? null : bodyHtml ? (
            // 后端已完成净化（body_html_sanitized），远程图片默认被阻止
            <div
              className="mt-4 text-[12.5px] leading-relaxed [&_a]:text-primary [&_img]:max-w-full"
              dangerouslySetInnerHTML={{ __html: bodyHtml }}
            />
          ) : (
            <pre className="mt-4 whitespace-pre-wrap break-words font-sans text-[12.5px] leading-relaxed">
              {bodyText || '（正文尚未缓存，点开时按需加载）'}
            </pre>
            )}
          </>
        ) : null}

        {preview ? (
          <div className="mt-4 rounded-lg border border-border bg-muted/40 p-2">
            <div className="mb-1 flex items-center justify-between gap-2">
              <span className="truncate text-[11px]">{preview.filename}</span>
              <button
                type="button"
                className="shrink-0 rounded px-1.5 py-0.5 text-[10.5px] text-muted-foreground hover:bg-black/5"
                onClick={() => setPreview(null)}
              >
                收起
              </button>
            </div>
            <img
              src={preview.dataUrl}
              alt={preview.filename}
              className="mx-auto max-h-[320px] max-w-full rounded object-contain"
            />
          </div>
        ) : null}

        {attachments.length ? (
          <div className="mt-5 border-t border-border pt-3">
            <p className="text-[11px] font-semibold">附件</p>
            <ul className="mt-1 space-y-1 text-[11px] text-muted-foreground">
              {attachments.map((attachment) => (
                <li key={`${attachment.filename}-${attachment.index}`} className="flex items-center gap-2">
                  <span className="min-w-0 flex-1 truncate">
                    {attachment.filename} · {attachment.mimeType || '未知类型'} ·{' '}
                    {Math.max(1, Math.round(attachment.sizeBytes / 1024))} KB
                  </span>
                  <button
                    type="button"
                    disabled={attachmentBusy === busyKey('preview', attachment.index)}
                    onClick={() => void previewAttachment(attachment.index)}
                    className="shrink-0 rounded px-1.5 py-0.5 text-[10.5px] text-primary hover:bg-primary/10 disabled:opacity-50"
                  >
                    {attachmentBusy === busyKey('preview', attachment.index) ? '…' : '预览'}
                  </button>
                  <button
                    type="button"
                    disabled={attachmentBusy === busyKey('download', attachment.index)}
                    onClick={() => void downloadAttachment(attachment.index)}
                    className="shrink-0 rounded px-1.5 py-0.5 text-[10.5px] text-primary hover:bg-primary/10 disabled:opacity-50"
                  >
                    {attachmentBusy === busyKey('download', attachment.index) ? '…' : '下载'}
                  </button>
                </li>
              ))}
            </ul>
          </div>
        ) : null}
      </div>
    </section>
  )
}
