import * as React from 'react'
import { useQuery } from '@tanstack/react-query'

import type {
  CorrespondentGroup,
  CorrespondentGroupCounts,
  CorrespondentListResult,
  CorrespondentThreadResult
} from '@renderer/shared/types'

const PAGE_SIZE = 40

/** 左栏两个分组 tab 的角标。 */
export function useCorrespondentGroupCounts(): {
  counts: CorrespondentGroupCounts | undefined
  loading: boolean
} {
  const query = useQuery({
    queryKey: ['correspondents', 'group-counts'],
    queryFn: () => window.api.correspondents.groupCounts(),
    staleTime: 15_000
  })
  return { counts: query.data, loading: query.isLoading }
}

/** 左栏联系人列表（按分组 + 关键词），支持游标翻页。 */
export function useCorrespondentList(group: CorrespondentGroup, keyword: string): {
  items: CorrespondentListResult['items']
  nextCursor: string | null
  loading: boolean
  loadingMore: boolean
  error: string | null
  loadMore: () => void
  refresh: () => void
} {
  const trimmed = keyword.trim()
  const [pages, setPages] = React.useState<CorrespondentListResult[]>([])
  const [loadingMore, setLoadingMore] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)

  const query = useQuery({
    queryKey: ['correspondents', 'list', group, trimmed],
    queryFn: () => window.api.correspondents.list({ group, keyword: trimmed, limit: PAGE_SIZE }),
    staleTime: 10_000
  })

  React.useEffect(() => {
    setPages(query.data ? [query.data] : [])
    setError(null)
  }, [query.data])

  const loadMore = React.useCallback(() => {
    const last = pages[pages.length - 1]
    const cursor = last?.nextCursor
    if (!cursor || loadingMore) return
    setLoadingMore(true)
    window.api.correspondents
      .list({ group, keyword: trimmed, cursor, limit: PAGE_SIZE })
      .then((page) => setPages((current) => [...current, page]))
      .catch((cause: unknown) => setError(cause instanceof Error ? cause.message : String(cause)))
      .finally(() => setLoadingMore(false))
  }, [group, loadingMore, pages, trimmed])

  const items = React.useMemo(() => pages.flatMap((page) => page.items), [pages])
  const nextCursor = pages[pages.length - 1]?.nextCursor ?? null

  return {
    items,
    nextCursor,
    loading: query.isLoading,
    loadingMore,
    error: error ?? (query.error instanceof Error ? query.error.message : null),
    loadMore,
    refresh: () => void query.refetch()
  }
}

/** 中栏：与某个联系人的全部往来（含垃圾与我的去信）。 */
export function useCorrespondentThread(correspondentId: number | null): {
  thread: CorrespondentThreadResult | undefined
  loading: boolean
  error: string | null
} {
  const query = useQuery({
    queryKey: ['correspondents', 'thread', correspondentId],
    queryFn: () => window.api.correspondents.thread(correspondentId as number, null, 200),
    enabled: correspondentId !== null,
    staleTime: 10_000
  })
  return {
    thread: query.data,
    loading: query.isLoading,
    error: query.error instanceof Error ? query.error.message : null
  }
}
