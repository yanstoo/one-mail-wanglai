/**
 * 邮件翻译（前端侧）。
 *
 * 交互参考沉浸式翻译：段落级、双语对照、结果缓存。
 * 翻译请求走 Rust 的 `translate_text`，不经过内置 AI，
 * 默认后端是 Google 公开端点，可在「设置 → 翻译」里换成
 * LibreTranslate / Azure / DeepL。
 */
import * as React from 'react'
import { toast } from 'sonner'

/** 模块级缓存：同一封邮件重复点开不重复请求。 */
const cache = new Map<string, string>()

export type TranslationTarget = 'body' | 'subject'

export function useTranslation(): {
  translate: (key: string, text: string) => Promise<void>
  translationOf: (key: string) => string | undefined
  pendingKey: string | null
} {
  const [pendingKey, setPendingKey] = React.useState<string | null>(null)
  const [, bump] = React.useReducer((value: number) => value + 1, 0)

  const translate = React.useCallback(async (key: string, text: string) => {
    const trimmed = text.trim()
    if (!trimmed) return
    // 已有译文 → 收起
    if (cache.has(key)) {
      cache.delete(key)
      bump()
      return
    }
    setPendingKey(key)
    try {
      const result = await window.api.translate.text({ text: trimmed })
      const value = result.text.trim()
      if (value) {
        cache.set(key, value)
        bump()
      }
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error))
    } finally {
      setPendingKey(null)
    }
  }, [])

  const translationOf = React.useCallback((key: string) => cache.get(key), [])

  return { translate, translationOf, pendingKey }
}

/** 中栏标题译文的缓存键。 */
export function subjectKey(source: string, messageId: number): string {
  return `subject:${source}:${messageId}`
}

/** 右栏正文译文的缓存键。 */
export function bodyKey(messageId: number): string {
  return `body:mail:${messageId}`
}
