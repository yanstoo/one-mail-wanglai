import type {
  CorrespondentBadge,
  CorrespondentGroup,
  CorrespondentSummary,
  CorrespondentThreadMessage
} from '@renderer/shared/types'

/** 徽标最多显示几个，其余折叠为 +N。 */
export const BADGE_VISIBLE_LIMIT = 2

/** 左栏一行展示的徽标：前 N 个 + 折叠数量。 */
export function visibleBadges(
  accounts: CorrespondentBadge[],
  badgeTotal: number,
  limit = BADGE_VISIBLE_LIMIT
): { shown: CorrespondentBadge[]; extra: number } {
  const shown = accounts.slice(0, limit)
  return { shown, extra: Math.max(0, badgeTotal - shown.length) }
}

/** 徽标 tooltip：全部账号（含未读），用于 +N 的 hover。 */
export function badgeTooltip(accounts: CorrespondentBadge[]): string {
  return accounts
    .map((account) => {
      const unread = account.unreadCount > 0 ? `（未读 ${account.unreadCount}）` : ''
      return `${account.badge} · ${account.email}${unread}`
    })
    .join('\n')
}

/** 分组角标取数：普通组取普通未读，垃圾组取垃圾未读，二者不相加。 */
export function groupUnreadCount(
  correspondent: CorrespondentSummary,
  group: CorrespondentGroup
): number {
  return group === 'junk' ? correspondent.junkUnreadCount : correspondent.unreadCount
}

/** 分组排序展示用时间：普通组 lastAt，垃圾组 junkLastAt。 */
export function groupLastAt(
  correspondent: CorrespondentSummary,
  group: CorrespondentGroup
): string | null {
  return group === 'junk' ? correspondent.junkLastAt : correspondent.lastAt
}

export function threadMessageKey(message: CorrespondentThreadMessage): string {
  return `${message.source}:${message.messageId}`
}

function dayKey(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value.slice(0, 10)
  return `${date.getFullYear()}/${date.getMonth() + 1}/${date.getDate()}`
}

/**
 * 中栏按日分组：入参是后端返回的倒序（新→旧），这里先反转成正序再按天切分，
 * 保证渲染顺序为「旧在上、新在下」，同一天的消息落进同一桶。
 */
export function groupThreadByDay(
  messages: CorrespondentThreadMessage[]
): { day: string; items: CorrespondentThreadMessage[] }[] {
  const ascending = [...messages].reverse()
  const buckets: { day: string; items: CorrespondentThreadMessage[] }[] = []
  for (const message of ascending) {
    const day = dayKey(message.sentAt)
    const last = buckets[buckets.length - 1]
    if (last && last.day === day) last.items.push(message)
    else buckets.push({ day, items: [message] })
  }
  return buckets
}

/** 常见二级后缀（用于判断 `xxx.com.cn` 这类注册域名取哪一段）。 */
const SECOND_LEVEL_SUFFIXES = new Set(['com', 'net', 'org', 'gov', 'edu', 'co', 'ac', 'or', 'ne', 'go', 'info', 'biz'])

/**
 * 从邮箱地址取「一级域名」作为徽标文字：
 * `a@mail.google.com` → `google`；`b@shop.example` → `shop`；
 * `c@x.com.cn` → `x`；`d@163.com` → `163`。
 */
export function domainLabel(address: string): string | null {
  const domain = (address.split('@')[1] ?? '').trim().toLowerCase()
  if (!domain) return null
  const parts = domain.split('.').filter(Boolean)
  if (parts.length <= 1) return parts[0] || null
  const tld = parts[parts.length - 1]
  const second = parts[parts.length - 2]
  const takeThird = tld.length === 2 && SECOND_LEVEL_SUFFIXES.has(second)
  const index = Math.max(parts.length - (takeThird ? 3 : 2), 0)
  return parts[index] || null
}

/** 自动徽标/头像的配色池：按首字母轮询，避免一屏全是同一颜色。 */
const AUTO_COLORS = [
  '#e5534b', '#e08a1e', '#c8a415', '#4c9a4c', '#2f8f8a',
  '#2d70d2', '#5a63c8', '#a0529f', '#c2557a', '#6b7f3a'
]

/** 按字符串首字母稳定映射到配色池（同一联系人每次颜色一致）。 */
export function autoColor(seed: string): string {
  const first = (seed.trim()[0] ?? '?').toUpperCase()
  const code = first.charCodeAt(0)
  // 字母按 A-Z 轮询，其它字符（数字/符号）也落进同一个池
  return AUTO_COLORS[(Number.isNaN(code) ? 0 : code) % AUTO_COLORS.length]
}
