import * as React from 'react'
import { AlertTriangle, ChevronDown } from 'lucide-react'

import { ContextMenu, useContextMenu } from '@renderer/components/context-menu'

import { formatRelativeTime } from '@renderer/components/mail/date-format'
import { cn } from '@renderer/lib/utils'
import type {
  CorrespondentGroup,
  CorrespondentGroupCounts,
  CorrespondentSummary
} from '@renderer/shared/types'

import { autoColor, domainLabel, groupLastAt, groupUnreadCount } from './correspondent-utils'

function initialOf(correspondent: CorrespondentSummary): string {
  const source = correspondent.displayName?.trim() || correspondent.address
  return source.slice(0, 1).toUpperCase()
}

function Avatar({ correspondent }: { correspondent: CorrespondentSummary }): React.JSX.Element {
  const initial = initialOf(correspondent)
  const first = correspondent.accounts[0]
  // 头像底色与自动徽标同一套「按首字母轮询」配色；账号徽标颜色不用这套
  const color = autoColor(initial)
  return (
    <span className="relative shrink-0">
      <span
        aria-hidden="true"
        className="flex size-9 items-center justify-center rounded-[10px] text-[14px] font-semibold text-white"
        style={{ backgroundColor: color }}
      >
        {initial}
      </span>
      {first ? (
        // 账号短徽标只占头像右下角一个小角，再长也不影响行内布局
        <span
          className="absolute -bottom-1 -right-1 max-w-[40px] truncate rounded-[4px] px-1 text-[8.5px] font-bold leading-[13px] text-white shadow-[0_0_0_1.5px_var(--background)]"
          style={{ backgroundColor: first.color ?? '#2d70d2' }}
          title={first.badge}
        >
          {first.badge}
        </span>
      ) : null}
    </span>
  )
}

/** 单独一行的徽标：域名徽标（自动配色）+ 其余账号徽标，右对齐。 */
function BadgeRow({ correspondent }: { correspondent: CorrespondentSummary }): React.JSX.Element | null {
  const domain = domainLabel(correspondent.address)
  const others = correspondent.accounts.slice(1)
  if (!domain && others.length === 0) return null

  return (
    <span className="mt-1 flex flex-wrap items-center justify-end gap-1">
      {domain ? (
        <span
          className="max-w-[64px] truncate rounded-[4px] px-1.5 text-[9px] font-semibold leading-[15px] text-white"
          style={{ backgroundColor: autoColor(domain) }}
          title={correspondent.address}
        >
          {domain}
        </span>
      ) : null}
      {others.map((account) => (
        <span
          key={account.accountId}
          className="max-w-[64px] truncate rounded-[4px] px-1.5 text-[9px] font-semibold leading-[15px] text-white"
          style={{ backgroundColor: account.color ?? '#2d70d2' }}
          title={account.badge}
        >
          {account.badge}
        </span>
      ))}
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
                'relative mb-0.5 flex w-full items-start gap-2.5 rounded-lg px-2 py-2 text-left',
                selected ? 'bg-background shadow-sm ring-1 ring-black/5 dark:ring-white/10' : 'hover:bg-white/60 dark:hover:bg-white/10'
              )}
            >
              {selected ? (
                <span className="absolute left-0 top-2 h-[calc(100%-1rem)] w-[3px] rounded-r bg-primary" aria-hidden="true" />
              ) : null}
              <Avatar correspondent={correspondent} />
              <span className="min-w-0 flex-1">
                <span className="flex items-baseline justify-between gap-1.5">
                  <span className="truncate text-[13px] font-semibold leading-5">
                    {correspondent.alias?.trim() || correspondent.displayName?.trim() || correspondent.address}
                  </span>
                  <span className="shrink-0 text-[10.5px] text-muted-foreground">
                    {lastAt ? formatRelativeTime(lastAt) : ''}
                  </span>
                </span>
                <span className="mt-0.5 flex items-center justify-between gap-2">
                  <span className="truncate text-[11.5px] leading-4 text-muted-foreground">
                    {correspondent.lastSubject?.trim() || correspondent.address}
                  </span>
                  <UnreadPill count={unread} tone={group === 'junk' ? 'junk' : 'normal'} />
                </span>
                <BadgeRow correspondent={correspondent} />
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
