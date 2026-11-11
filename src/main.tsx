import './assets/main.css'

import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import App from './App'
import { createDesktopApi } from './lib/desktop-api'
import { initializeAppearance } from './lib/appearance'

window.api = createDesktopApi()

// 屏蔽 WebView 默认右键菜单（重新加载 / 检查元素等开发项）。
// 例外：输入类控件内、以及有选中文字时保留原生菜单，方便复制粘贴。
document.addEventListener('contextmenu', (event) => {
  const target = event.target as HTMLElement | null
  if (target?.closest('input, textarea, [contenteditable="true"]')) return
  const selection = window.getSelection()
  if (selection && !selection.isCollapsed && selection.toString().trim().length > 0) return
  event.preventDefault()
})

document.documentElement.dataset.platform = getPlatform()

void initializeAppearance().then(() => {
  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <App />
    </StrictMode>
  )
})

function getPlatform(): 'macos' | 'windows' | 'linux' {
  const platform = navigator.platform.toLowerCase()
  if (platform.includes('mac')) return 'macos'
  if (platform.includes('win')) return 'windows'
  return 'linux'
}
