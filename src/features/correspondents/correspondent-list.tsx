import * as React from 'react'
import { AlertTriangle, ChevronDown } from 'lucide-react'

import { ContextMenu, useContextMenu } from '@renderer/components/context-menu'

import { formatRelativeTime } from '@renderer/components/mail/date-format'
import { cn } from '@renderer/lib/utils'
import type {
  CorrespondentBadge,
  CorrespondentGroup,
  CorrespondentGroupCounts,
  CorrespondentSummary
} from '@renderer/shared/types'

import { badgeTooltip, groupLastAt, groupUnreadCount, visibleBadges } from './correspondent-utils'

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
    // relative：徽标绝对定位在头像右下角，徽标再长也不挤压行内布局
    <span className="relative shrink-0">
      <span
        aria-hidden="true"
        className="flex size-8 items-center justify-center rounded-full text-[13px] font-semibold text-white"
        style={{ backgroundColor: color }}
      >
        {initialOf(correspondent)}
      </span>
      <BadgeStack correspondent={correspondent} />
    </span>
  )
}

/** 徽标：最多 2 个，溢出用 +N，hover 展示全部账号。 */
function BadgeStack({ correspondent }: { correspondent: CorrespondentSummary }): React.JSX.Element | null {
  const { accounts, badgeTotal } = correspondent
  if (accounts.length === 0) return null
  const { shown, extra } = visibleBadges(accounts, badgeTotal)

  return (
    <span
      className="absolute -bottom-1 left-1/2 flex max-w-[72px] -translate-x-1/2 items-center gap-0.5"
      title={badgeTooltip(accounts)}
    >
      {shown.map((account, index) => (
        <span
          key={account.accountId}
          className="max-w-[52px] truncate rounded-[3px] px-1 text-[8px] font-bold leading-[13px] text-white shadow-[0_0_0_1px_var(--background)]"
          style={{ backgroundColor: badgeColor(account, index) }}
        >
          {account.badge}
        </span>
      ))}
      {extra > 0 ? (
        <span className="rounded-[3px] bg-muted px-1 text-[8px] font-semibold leading-[13px] text-muted-foreground shadow-[0_0_0_1px_var(--background)]">
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
  items,
  selectedId,
  onSelect,
  loading,
  nextCursor,
  loadMore,
  accounts,
  accountId,
  onAccountChange,
  onRename,
  onClearAlias,
  searching
}: {
  group: CorrespondentGroup
  onGroupChange: (group: CorrespondentGroup) => void
  counts: CorrespondentGroupCounts | undefined
  items: CorrespondentSummary[]
  selectedId: number | null
  onSelect: (correspondentId: number) => void
  loading: boolean
  nextCursor: string | null
  loadMore: () => void
  accounts: { accountId: number; label: string; badge: string }[]
  accountId: number | null
  onAccountChange: (accountId: number | null) => void
  onRename: (correspondent: CorrespondentSummary) => void
  onClearAlias: (correspondent: CorrespondentSummary) => void
  searching: boolean
}): React.JSX.Element {
  const { menu, open, close } = useContextMenu()
  return (
    <aside className="native-sidebar flex min-h-0 w-[264px] shrink-0 flex-col border-r border-black/5 text-xs dark:border-white/8">
      <div className="flex h-10 shrink-0 items-center justify-between px-3">
        <div className="flex items-baseline gap-1.5">
          <span className="text-[13px] font-semibold">往来联系人</span>
          <span className="text-[11px] text-muted-foreground">
            {counts ? counts[group].contacts : '—'}
          </span>
        </div>
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
        <div className="relative">
          <select
            value={accountId ?? ''}
            onChange={(event) =>
              onAccountChange(event.target.value === '' ? null : Number(event.target.value))
            }
            className="w-full appearance-none rounded-lg border border-black/10 bg-background px-2 py-1.5 pr-6 text-[11.5px] outline-none dark:border-white/15"
            aria-label="按邮箱筛选"
          >
            <option value="">全部邮箱</option>
            {accounts.map((account) => (
              <option key={account.accountId} value={account.accountId}>
                {account.badge} · {account.label}
              </option>
            ))}
          </select>
          <ChevronDown
            className="pointer-events-none absolute right-2 top-1/2 size-3 -translate-y-1/2 text-muted-foreground"
            aria-hidden="true"
          />
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-1.5 pb-2">
        {loading && items.length === 0 ? (
          <p className="px-2 py-6 text-center text-[11px] text-muted-foreground">正在载入…</p>
        ) : null}
        {!loading && items.length === 0 ? (
          <p className="px-2 py-6 text-center text-[11px] text-muted-foreground">
            {searching ? '没有匹配的联系人' : '还没有往来记录'}
          </p>
        ) : null}

        {items.map((correspondent) => {
          const selected = correspondent.correspondentId === selectedId
          const unread = groupUnreadCount(correspondent, group)
          const lastAt = groupLastAt(correspondent, group)
          return (
            <button
              key={correspondent.correspondentId}
              type="button"
              onClick={() => onSelect(correspondent.correspondentId)}
              onContextMenu={(event) =>
                open(event, [
                  {
                    label: correspondent.alias ? '修改备注名…' : '设置备注名…',
                    onSelect: () => onRename(correspondent)
                  },
                  {
                    label: '清除备注名',
                    disabled: !correspondent.alias,
                    onSelect: () => onClearAlias(correspondent)
                  }
                ])
              }
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
                    {correspondent.alias?.trim() || correspondent.displayName?.trim() || correspondent.address}
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
            <AlertTriangle className="size-3" aria-hidden="true" /> 载入更多
          </button>
        ) : null}
      </div>
      <ContextMenu menu={menu} onClose={close} />
    </aside>
  )
}
