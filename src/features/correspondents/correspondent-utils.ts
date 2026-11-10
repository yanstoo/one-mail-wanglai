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
