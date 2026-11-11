import * as React from 'react'

import { cn } from '@renderer/lib/utils'

export type ContextMenuItem = {
  label: string
  onSelect: () => void
  /** 危险操作（删除类）会用红色文字 */
  danger?: boolean
  disabled?: boolean
}

export type ContextMenuState = {
  x: number
  y: number
  items: ContextMenuItem[]
} | null

/** 打开右键菜单：自动避开窗口右/下边界。 */
export function useContextMenu(): {
  menu: ContextMenuState
  open: (event: React.MouseEvent, items: ContextMenuItem[]) => void
  close: () => void
} {
  const [menu, setMenu] = React.useState<ContextMenuState>(null)
  const open = React.useCallback((event: React.MouseEvent, items: ContextMenuItem[]) => {
    event.preventDefault()
    event.stopPropagation()
    const width = 180
    const height = items.length * 30 + 12
    setMenu({
      x: Math.min(event.clientX, window.innerWidth - width - 8),
      y: Math.min(event.clientY, window.innerHeight - height - 8),
      items
    })
  }, [])
  const close = React.useCallback(() => setMenu(null), [])
  return { menu, open, close }
}

/** 通用右键菜单浮层。 */
export function ContextMenu({
  menu,
  onClose
}: {
  menu: ContextMenuState
  onClose: () => void
}): React.JSX.Element | null {
  React.useEffect(() => {
    if (!menu) return
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [menu, onClose])

  if (!menu) return null
  return (
    <>
      <button
        type="button"
        aria-label="关闭菜单"
        className="fixed inset-0 z-[60] cursor-default"
        onClick={onClose}
        onContextMenu={(event) => {
          event.preventDefault()
          onClose()
        }}
      />
      <div
        role="menu"
        style={{ left: menu.x, top: menu.y }}
        className="fixed z-[61] min-w-[168px] overflow-hidden rounded-lg border border-border bg-popover p-1 text-[12px] shadow-lg"
      >
        {menu.items.map((item) => (
          <button
            key={item.label}
            type="button"
            role="menuitem"
            disabled={item.disabled}
            onClick={() => {
              onClose()
              item.onSelect()
            }}
            className={cn(
              'flex w-full items-center rounded-md px-2 py-1.5 text-left hover:bg-muted disabled:opacity-40',
              item.danger ? 'text-destructive' : ''
            )}
          >
            {item.label}
          </button>
        ))}
      </div>
    </>
  )
}
