#!/usr/bin/env node
/**
 * 修补 Tauri 产出的 deb 包依赖声明。
 *
 * 为什么需要：Tauri 的 `bundle.linux.deb.depends` 是**追加**到自动探测结果之后，
 * 无法覆盖它。自动探测在 Ubuntu 22.04 上给出 `libgtk-3-0`，而 Ubuntu 24.04+ 与
 * Debian 13 把它改名成 `libgtk-3-0t64`，导致这些发行版上 `apt install` 直接失败：
 *   「依赖: libgtk-3-0 但无法安装它」
 *
 * 做法：解包控制文件 → 把 Depends 重写成去重且带备选的列表 → 重新打包。
 *
 * 用法：node scripts/patch-deb-depends.mjs [deb 路径]
 *   不传路径时在 src-tauri/target 下递归查找最新的 *.deb。
 *   带 `--target <triple>` 构建时产物在
 *   src-tauri/target/<triple>/release/bundle/deb/，而不是 target/release/...
 */
import { execFileSync } from 'node:child_process'
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

/** 新旧发行版都兼容的依赖写法；顺序稳定便于 diff。 */
const WANTED = [
  'libwebkit2gtk-4.1-0',
  'libjavascriptcoregtk-4.1-0',
  'libsoup-3.0-0',
  'libayatana-appindicator3-1',
  'libgtk-3-0t64 | libgtk-3-0'
]

function collectDebs(dir, depth = 0) {
  if (depth > 6 || !existsSync(dir)) return []
  const found = []
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name)
    if (entry.isDirectory()) found.push(...collectDebs(full, depth + 1))
    else if (entry.name.endsWith('.deb')) found.push(full)
  }
  return found
}

function findDeb() {
  const root = join('src-tauri', 'target')
  const files = collectDebs(root)
  if (files.length === 0) {
    throw new Error(`未找到任何 deb 产物（${root} 下为空）：先跑 pnpm tauri build --bundles deb`)
  }
  // 必须按修改时间取最新：版本号是 YY.MMDD.HHMM，字符串排序会挑到旧包
  const newest = files
    .map((path) => ({ path, mtime: statSync(path).mtimeMs }))
    .sort((left, right) => right.mtime - left.mtime)[0]
  return newest.path
}

function run(command, args) {
  return execFileSync(command, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] })
}

const debPath = process.argv[2] ?? findDeb()
const workDir = mkdtempSync(join(tmpdir(), 'onemail-deb-'))

try {
  // 1. 解开控制文件
  run('dpkg-deb', ['-R', debPath, workDir])
  const controlPath = join(workDir, 'DEBIAN', 'control')
  const control = readFileSync(controlPath, 'utf8')

  // 2. 重写 Depends（保留其它字段原样）
  const lines = control.split('\n')
  const index = lines.findIndex((line) => line.startsWith('Depends:'))
  const dependsLine = `Depends: ${WANTED.join(', ')}`
  const next = [...lines]
  if (index >= 0) next.splice(index, 1, dependsLine)
  else next.splice(1, 0, dependsLine)
  writeFileSync(controlPath, next.join('\n'))

  // 3. 重新打包（先删旧包，避免 dpkg-deb 报已存在）
  const previous = run('dpkg-deb', ['-f', debPath, 'Depends']).trim()
  rmSync(debPath)
  run('dpkg-deb', ['--root-owner-group', '-b', workDir, debPath])

  console.log(`已修补 ${debPath}`)
  console.log(`  旧 Depends: ${previous}`)
  console.log(`  新 Depends: ${run('dpkg-deb', ['-f', debPath, 'Depends']).trim()}`)
} finally {
  rmSync(workDir, { recursive: true, force: true })
}
