import * as React from 'react'
import { Check, Search, AlertTriangle } from 'lucide-react'

import { formatRelativeTime } from '@renderer/components/mail/date-format'
import { cn } from '@renderer/lib/utils'
import type {
  CorrespondentBadge,
  CorrespondentGroup,
  CorrespondentGroupCounts,
  CorrespondentSummary
} from '@renderer/shared/types'

const BADGE_FALLBACK_COLORS = ['#2d70d2', '#ea4335', '#12b7f5', '#ff6a00', '#7b61c9', '#227a4b']

function badgeColor(badge: CorrespondentBadge, index: number): string {
  return badge.color ?? BADGE_FALLBACK_COLORS[index % BADGE_FALLBACK_COLORS.length]
}

function initialOf(correspondent: CorrespondentSummary): string {
  const source = correspondent.displayName?.trim() || correspondent.address
  return source.slice(0, 1).toUpperCase()
}

function Avatar({ correspondent }: { correspondent: CorrespondentSummary }): React.JSX.Element {
  const first = correspondent.accounts[0]
  const color = first ? badgeColor(first, 0) : '#2d70d2'
  return (
    <span
      aria-hidden="true"
      className="flex size-8 shrink-0 items-center justify-center rounded-full text-[13px] font-semibold text-white"
      style={{ backgroundColor: color }}
    >
      {initialOf(correspondent)}
    </span>
  )
}

/** 徽标：最多 2 个，溢出用 +N，hover 展示全部。 */
function BadgeStack({ correspondent }: { correspondent: CorrespondentSummary }): React.JSX.Element | null {
  const { accounts, badgeTotal } = correspondent
  if (accounts.length === 0) return null
  const shown = accounts.slice(0, 2)
  const extra = Math.max(0, badgeTotal - shown.length)
  const tooltip = accounts
    .map((account) => `${account.badge} · ${account.email}${account.unreadCount > 0 ? `（未读 ${account.unreadCount}）` : ''}`)
    .join('\n')

  return (
    <span className="flex items-center gap-0.5" title={tooltip}>
      {shown.map((account, index) => (
        <span
          key={account.accountId}
          className="rounded-[4px] px-1 text-[9px] font-bold leading-4 text-white"
          style={{ backgroundColor: badgeColor(account, index) }}
        >
          {account.badge}
        </span>
      ))}
      {extra > 0 ? (
        <span className="rounded-[4px] bg-muted px-1 text-[9px] font-semibold leading-4 text-muted-foreground">
          +{extra}
        </span>
      ) : null}
    </span>
  )
}

function UnreadPill({ count, tone }: { count: number; tone: 'normal' | 'junk' }): React.JSX.Element | null {
  if (count <= 0) return null
  return (
    <span
      className={cn(
        'flex h-4 min-w-4 items-center justify-center rounded-full px-1 text-[10px] font-bold text-white',
        tone === 'junk' ? 'bg-[#9a9aa4]' : 'bg-destructive'
      )}
    >
      {count > 99 ? '99+' : count}
    </span>
  )
}

export function CorrespondentList({
  group,
  onGroupChange,
  counts,
  keyword,
  onKeywordChange,
  items,
  selectedId,
  onSelect,
  loading,
  nextCursor,
  loadMore
}: {
  group: CorrespondentGroup
  onGroupChange: (group: CorrespondentGroup) => void
  counts: CorrespondentGroupCounts | undefined
  keyword: string
  onKeywordChange: (keyword: string) => void
  items: CorrespondentSummary[]
  selectedId: number | null
  onSelect: (correspondentId: number) => void
  loading: boolean
  nextCursor: string | null
  loadMore: () => void
}): React.JSX.Element {
  return (
    <aside className="native-sidebar flex min-h-0 w-[264px] shrink-0 flex-col border-r border-black/5 text-xs dark:border-white/8">
      <div className="flex h-10 shrink-0 items-center justify-between px-3">
        <div className="flex items-baseline gap-1.5">
          <span className="text-[13px] font-semibold">往来联系人</span>
          <span className="text-[11px] text-muted-foreground">
            {counts ? counts[group].contacts : '—'}
          </span>
        </div>
        <Search className="size-3.5 text-muted-foreground" aria-hidden="true" />
      </div>

      <div className="mx-3 mb-2 flex shrink-0 rounded-lg bg-black/5 p-0.5 dark:bg-white/10">
        {([
          ['normal', '普通', counts?.normal.unread ?? 0, 'bg-destructive'],
          ['junk', '垃圾', counts?.junk.unread ?? 0, 'bg-[#9a9aa4]']
        ] as const).map(([value, label, unread, dot]) => (
          <button
            key={value}
            type="button"
            onClick={() => onGroupChange(value)}
            className={cn(
              'flex flex-1 items-center justify-center gap-1 rounded-md py-1 text-[11.5px]',
              group === value
                ? 'bg-background font-semibold shadow-sm'
                : 'text-muted-foreground hover:text-foreground'
            )}
            aria-pressed={group === value}
          >
            {label}
            {unread > 0 ? (
              <span className={cn('flex h-3.5 min-w-3.5 items-center justify-center rounded-full px-1 text-[9.5px] font-bold text-white', dot)}>
                {unread > 99 ? '99+' : unread}
              </span>
            ) : null}
          </button>
        ))}
      </div>

      <div className="mx-3 mb-2 shrink-0">
        <div className="flex items-center gap-1.5 rounded-lg border border-black/10 bg-background px-2 py-1.5 dark:border-white/15">
          <Search className="size-3 text-muted-foreground" aria-hidden="true" />
          <input
            value={keyword}
            onChange={(event) => onKeywordChange(event.target.value)}
            placeholder="搜索地址或名称"
            className="w-full bg-transparent text-[11.5px] outline-none placeholder:text-muted-foreground"
          />
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-1.5 pb-2">
        {loading && items.length === 0 ? (
          <p className="px-2 py-6 text-center text-[11px] text-muted-foreground">正在载入…</p>
        ) : null}
        {!loading && items.length === 0 ? (
          <p className="px-2 py-6 text-center text-[11px] text-muted-foreground">
            {keyword.trim() ? '没有匹配的联系人' : '还没有往来记录'}
          </p>
        ) : null}

        {items.map((correspondent) => {
          const selected = correspondent.correspondentId === selectedId
          const unread = group === 'junk' ? correspondent.junkUnreadCount : correspondent.unreadCount
          const lastAt = group === 'junk' ? correspondent.junkLastAt : correspondent.lastAt
          return (
            <button
              key={correspondent.correspondentId}
              type="button"
              onClick={() => onSelect(correspondent.correspondentId)}
              className={cn(
                'relative mb-0.5 flex w-full items-start gap-2 rounded-lg px-1.5 py-1.5 text-left',
                selected ? 'bg-background shadow-sm ring-1 ring-black/5 dark:ring-white/10' : 'hover:bg-white/60 dark:hover:bg-white/10'
              )}
            >
              {selected ? (
                <span className="absolute left-0 top-2 h-[calc(100%-1rem)] w-[3px] rounded-r bg-primary" aria-hidden="true" />
              ) : null}
              <Avatar correspondent={correspondent} />
              <span className="min-w-0 flex-1">
                <span className="flex items-center justify-between gap-1">
                  <span className="truncate text-[12.5px] font-semibold">
                    {correspondent.displayName?.trim() || correspondent.address}
                  </span>
                  <span className="flex shrink-0 items-center gap-1">
                    <UnreadPill count={unread} tone={group === 'junk' ? 'junk' : 'normal'} />
                    <span className="text-[10.5px] text-muted-foreground">
                      {lastAt ? formatRelativeTime(lastAt) : ''}
                    </span>
                  </span>
                </span>
                <span className="mt-0.5 flex items-center justify-between gap-1">
                  <span className="truncate text-[11px] text-muted-foreground">
                    {correspondent.lastSubject?.trim() || correspondent.address}
                  </span>
                  <BadgeStack correspondent={correspondent} />
                </span>
              </span>
            </button>
          )
        })}

        {nextCursor ? (
          <button
            type="button"
            onClick={loadMore}
            className="mt-1 flex w-full items-center justify-center gap-1 rounded-lg py-1.5 text-[11px] text-muted-foreground hover:bg-white/60 dark:hover:bg-white/10"
          >
            <Check className="size-3" aria-hidden="true" /> 载入更多
          </button>
        ) : null}
      </div>

      <div className="flex shrink-0 items-center gap-1 border-t border-black/5 px-3 py-1.5 text-[10.5px] text-muted-foreground dark:border-white/8">
        <AlertTriangle className="size-3" aria-hidden="true" />
        垃圾组只是视图：中栏内容与普通组一致
      </div>
    </aside>
  )
}
