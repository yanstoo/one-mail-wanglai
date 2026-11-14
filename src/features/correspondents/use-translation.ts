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

/** 段落切分：按空行分段，保留段内换行。用于「原文 + 译文」逐段对照。 */
export function splitParagraphs(text: string): string[] {
  return text
    .replace(/\r\n/g, '\n')
    .split(/\n{2,}/)
    .map((value) => value.trim())
    .filter((value) => value.length > 0)
}

/**
 * 逐段翻译：每段单独请求并缓存（key = 邮件 id + 段序号）。
 * 并发 3 个，段数上限 40，避免一封长邮件打出上百个请求。
 */
export function useParagraphTranslation(): {
  translateParagraphs: (messageId: number, paragraphs: string[]) => Promise<void>
  paragraphsOf: (messageId: number) => string[] | undefined
  pending: boolean
  clear: (messageId: number) => void
} {
  const [pending, setPending] = React.useState(false)
  const [, bump] = React.useReducer((value: number) => value + 1, 0)

  const translateParagraphs = React.useCallback(
    async (messageId: number, paragraphs: string[]) => {
      const limited = paragraphs.slice(0, 40)
      setPending(true)
      try {
        for (let index = 0; index < limited.length; index += 3) {
          const batch = limited.slice(index, index + 3)
          const results = await Promise.all(
            batch.map(async (text, offset) => {
              const key = `para:${messageId}:${index + offset}`
              if (cache.has(key)) return cache.get(key) ?? ''
              try {
                const result = await window.api.translate.text({ text })
                const value = result.text.trim()
                cache.set(key, value)
                return value
              } catch {
                return ''
              }
            })
          )
          // 单段失败不影响其它段
          results.forEach((value, offset) => {
            if (value) cache.set(`para:${messageId}:${index + offset}`, value)
          })
        }
        bump()
      } finally {
        setPending(false)
      }
    },
    []
  )

  const paragraphsOf = React.useCallback((messageId: number) => {
    const values: string[] = []
    for (let index = 0; index < 40; index += 1) {
      const value = cache.get(`para:${messageId}:${index}`)
      if (value === undefined) break
      values.push(value)
    }
    return values.length > 0 ? values : undefined
  }, [])

  const clear = React.useCallback((messageId: number) => {
    for (let index = 0; index < 40; index += 1) cache.delete(`para:${messageId}:${index}`)
    bump()
  }, [])

  return { translateParagraphs, paragraphsOf, pending, clear }
}
