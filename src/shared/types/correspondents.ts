/** 往来联系人索引相关类型（对应 Rust `db::correspondents` 的输出）。 */

export type CorrespondentGroup = 'normal' | 'junk'

export type CorrespondentBadge = {
  accountId: number
  label: string
  email: string
  /** 短徽标（≤2 字），如 G / QQ / 阿 */
  badge: string
  /** 徽标颜色（HEX），未配置时为 null，由前端回落到调色板 */
  color: string | null
  lastAt: string | null
  unreadCount: number
  junkUnreadCount: number
}

export type CorrespondentSummary = {
  correspondentId: number
  address: string
  addressDisplay: string
  displayName: string | null
  lastAt: string | null
  junkLastAt: string | null
  /** 普通未读（普通组红角标） */
  unreadCount: number
  /** 垃圾未读（垃圾组灰角标），与上面不相加 */
  junkUnreadCount: number
  inboundCount: number
  outboundCount: number
  lastSubject: string | null
  lastIsJunk: boolean
  lastHasAttachment: boolean
  badgeTotal: number
  accounts: CorrespondentBadge[]
}

export type CorrespondentProfile = Omit<
  CorrespondentSummary,
  'badgeTotal' | 'lastSubject' | 'lastIsJunk' | 'lastHasAttachment'
>

export type CorrespondentListQuery = {
  group: CorrespondentGroup
  keyword?: string
  cursor?: string
  limit?: number
  /** 只看某个「我的邮箱」下的往来；不传=全部邮箱 */
  accountId?: number
}

export type CorrespondentListResult = {
  group: CorrespondentGroup
  items: CorrespondentSummary[]
  nextCursor: string | null
}

export type CorrespondentThreadMessage = {
  source: 'mail' | 'outbox'
  messageId: number
  accountId: number
  direction: 'in' | 'out'
  sentAt: string
  subject: string | null
  isRead: boolean
  hasAttachment: boolean
  /** 落在垃圾文件夹里 */
  isJunk: boolean
}

export type CorrespondentThreadResult = {
  correspondent: CorrespondentProfile
  /** 按时间倒序（新→旧），渲染时反转 */
  messages: CorrespondentThreadMessage[]
  nextCursor: string | null
}

export type CorrespondentGroupCounts = {
  normal: { contacts: number; unread: number }
  junk: { contacts: number; unread: number }
}
