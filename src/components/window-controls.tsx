import * as React from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'

/**
 * 自绘窗口控制按钮（macOS 之外的平台）。
 *
 * Linux 下主窗口用 `decorations: false`（见 src-tauri/tauri.linux.conf.json），
 * 系统标题栏被移除，这里按设计稿画左上角三颗圆点：关闭 / 最小化 / 最大化。
 * macOS 保留系统原生交通灯（titleBarStyle: Overlay），不渲染本组件。
 */
export function WindowControls(): React.JSX.Element | null {
  const platform = typeof document !== 'undefined' ? document.documentElement.dataset.platform : undefined
  const [maximized, setMaximized] = React.useState(false)

  React.useEffect(() => {
    if (platform === 'macos') return
    const win = getCurrentWindow()
    let disposed = false
    void win.isMaximized().then((value) => {
      if (!disposed) setMaximized(value)
    })
    const unlisten = win.onResized(() => {
      void win.isMaximized().then((value) => {
        if (!disposed) setMaximized(value)
      })
    })
    return () => {
      disposed = true
      void unlisten.then((fn) => fn())
    }
  }, [platform])

  if (platform === 'macos') return null

  const win = getCurrentWindow()
  const run = (action: () => Promise<unknown>): void => {
    void action().catch((error: unknown) => {
      console.warn('window control failed', error)
    })
  }

  return (
    <div className="app-no-drag flex shrink-0 items-center gap-2" aria-label="窗口控制">
      {/* Linux / Windows 约定：最小化 → 最大化 → 关闭（关闭在最右） */}
      <button
        type="button"
        aria-label="最小化"
        title="最小化"
        onClick={() => run(() => win.minimize())}
        className="group flex size-3 items-center justify-center rounded-full bg-[#febc2e] hover:brightness-95"
      >
        <span className="text-[7px] font-bold leading-none text-black/50 opacity-0 transition-opacity group-hover:opacity-100">
          −
        </span>
      </button>
      <button
        type="button"
        aria-label={maximized ? '还原' : '最大化'}
        title={maximized ? '还原' : '最大化'}
        onClick={() => run(() => (maximized ? win.unmaximize() : win.maximize()))}
        className="group flex size-3 items-center justify-center rounded-full bg-[#28c840] hover:brightness-95"
      >
        <span className="text-[7px] font-bold leading-none text-black/50 opacity-0 transition-opacity group-hover:opacity-100">
          +
        </span>
      </button>
      <button
        type="button"
        aria-label="关闭"
        title="关闭"
        onClick={() => run(() => win.close())}
        className="group flex size-3 items-center justify-center rounded-full bg-[#ff5f57] hover:brightness-95"
      >
        <span className="text-[7px] font-bold leading-none text-black/50 opacity-0 transition-opacity group-hover:opacity-100">
          ×
        </span>
      </button>
    </div>
  )
}
