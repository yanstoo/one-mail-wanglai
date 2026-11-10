import { describe, expect, it } from 'vitest'

import type { CorrespondentBadge, CorrespondentSummary, CorrespondentThreadMessage } from '@renderer/shared/types'

import {
  badgeTooltip,
  groupLastAt,
  groupThreadByDay,
  groupUnreadCount,
  threadMessageKey,
  visibleBadges
} from './correspondent-utils'

function badge(overrides: Partial<CorrespondentBadge>): CorrespondentBadge {
  return {
    accountId: 1,
    label: '主邮箱',
    email: 'me@a.test',
    badge: 'G',
    color: '#ea4335',
    lastAt: '2026-11-01T00:00:00.000Z',
    unreadCount: 0,
    junkUnreadCount: 0,
    ...overrides
  }
}

function correspondent(overrides: Partial<CorrespondentSummary>): CorrespondentSummary {
  return {
    correspondentId: 1,
    address: 'alice@corp.test',
    addressDisplay: 'alice@corp.test',
    displayName: 'Alice',
    lastAt: '2026-11-01T09:00:00.000Z',
    junkLastAt: null,
    unreadCount: 2,
    junkUnreadCount: 5,
    inboundCount: 3,
    outboundCount: 1,
    lastSubject: '项目排期',
    lastIsJunk: false,
    lastHasAttachment: false,
    badgeTotal: 1,
    accounts: [badge({})],
    ...overrides
  }
}

describe('visibleBadges', () => {
  it('最多显示 2 个徽标', () => {
    const accounts = [badge({ accountId: 1 }), badge({ accountId: 2, badge: 'QQ' })]
    expect(visibleBadges(accounts, 2)).toEqual({ shown: accounts, extra: 0 })
  })

  it('超出部分折叠成 +N', () => {
    const accounts = [
      badge({ accountId: 1 }),
      badge({ accountId: 2, badge: 'QQ' }),
      badge({ accountId: 3, badge: '阿' }),
      badge({ accountId: 4, badge: 'O' })
    ]
    const result = visibleBadges(accounts, 4)
    expect(result.shown).toHaveLength(2)
    expect(result.extra).toBe(2)
  })

  it('badgeTotal 小于账号数时以账号数为准，不出现负数', () => {
    const result = visibleBadges([badge({})], 0)
    expect(result.extra).toBe(0)
  })
})

describe('badgeTooltip', () => {
  it('列出全部账号，带未读时标注', () => {
    const tooltip = badgeTooltip([
      badge({ badge: 'G', email: 'me@a.test', unreadCount: 3 }),
      badge({ accountId: 2, badge: 'QQ', email: 'me@b.test', unreadCount: 0 })
    ])
    expect(tooltip).toBe('G · me@a.test（未读 3）\nQQ · me@b.test')
  })
})

describe('分组取数', () => {
  it('普通组取普通未读与 lastAt', () => {
    const item = correspondent({})
    expect(groupUnreadCount(item, 'normal')).toBe(2)
    expect(groupLastAt(item, 'normal')).toBe('2026-11-01T09:00:00.000Z')
  })

  it('垃圾组取垃圾未读与 junkLastAt，且与普通未读不相加', () => {
    const item = correspondent({ junkLastAt: '2026-11-09T09:00:00.000Z' })
    expect(groupUnreadCount(item, 'junk')).toBe(5)
    expect(groupLastAt(item, 'junk')).toBe('2026-11-09T09:00:00.000Z')
  })
})

describe('threadMessageKey', () => {
  it('区分 mail 与 outbox 两套 id 空间', () => {
    const mail = { source: 'mail', messageId: 7 } as CorrespondentThreadMessage
    const outbox = { source: 'outbox', messageId: 7 } as CorrespondentThreadMessage
    expect(threadMessageKey(mail)).toBe('mail:7')
    expect(threadMessageKey(outbox)).toBe('outbox:7')
    expect(threadMessageKey(mail)).not.toBe(threadMessageKey(outbox))
  })
})

describe('groupThreadByDay', () => {
  const message = (
    messageId: number,
    sentAt: string,
    overrides: Partial<CorrespondentThreadMessage> = {}
  ): CorrespondentThreadMessage => ({
    source: 'mail',
    messageId,
    accountId: 1,
    direction: 'in',
    sentAt,
    subject: `s${messageId}`,
    isRead: true,
    hasAttachment: false,
    isJunk: false,
    ...overrides
  })

  it('把倒序输入反转成正序，并保留垃圾标记', () => {
    const buckets = groupThreadByDay([
      message(3, '2026-11-05T12:00:00.000Z', { isJunk: true }),
      message(2, '2026-11-01T09:00:00.000Z'),
      message(1, '2026-10-29T09:00:00.000Z')
    ])
    expect(buckets.map((bucket) => bucket.day)).toEqual(['2026/10/29', '2026/11/1', '2026/11/5'])
    expect(buckets[2].items[0].isJunk).toBe(true)
  })

  it('同一天的消息落进同一个桶并保持时间顺序', () => {
    // 注意：按本地时区分日。这里用 01:00Z / 09:00Z，在 Asia/Shanghai 分别是 09:00 / 17:00 同一天。
    const buckets = groupThreadByDay([
      message(2, '2026-11-01T09:00:00.000Z'),
      message(1, '2026-11-01T01:00:00.000Z')
    ])
    expect(buckets).toHaveLength(1)
    expect(buckets[0].items.map((item) => item.messageId)).toEqual([1, 2])
  })

  it('空数组返回空分桶', () => {
    expect(groupThreadByDay([])).toEqual([])
  })

  it('异常时间串退化为按前 10 个字符分组，不抛错', () => {
    const buckets = groupThreadByDay([message(1, 'not-a-date')])
    expect(buckets[0].day).toBe('not-a-date')
  })
})
