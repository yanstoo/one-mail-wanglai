# 发布流程（Release）

本仓库是 [OneMail](https://github.com/zhihui-hu/one-mail) 的非官方分支，按 AGPL-3.0-only 分发。
发布通过 GitHub Actions 完成：打 `v*` 标签即触发 `.github/workflows/release.yml`。

## 一、一次性准备

1. **确认 workflow 有写权限**
   `release.yml` 的 `prepare` / `build` 作业各自声明了 `permissions: contents: write`，
   所以不需要改仓库的默认 Workflow 权限。只需确认仓库 **Actions 已启用**。

2. **配置 remote**（若还没有 upstream）

   ```bash
   git remote -v                                   # 应看到 origin 指向自己的仓库
   git remote add upstream git@github.com:zhihui-hu/one-mail.git   # 只用于 cherry-pick，可选
   ```

3. **不需要配置任何 secrets 就能出包**。以下都是可选项：

   | Secret | 作用 | 不配的后果 |
   |---|---|---|
   | `APPLE_CERTIFICATE` / `APPLE_CERTIFICATE_PASSWORD` / `APPLE_ID` 等 | macOS 正式签名 + 公证 | 用 ad-hoc 签名（工作流已设 `APPLE_SIGNING_IDENTITY='-'`），用户首次打开需右键「打开」或 `xattr -dr com.apple.quarantine` |
   | `WINDOWS_CERTIFICATE` 等 | Windows 代码签名 | SmartScreen 首次运行会提示「未知发布者」 |
   | `TAURI_SIGNING_PRIVATE_KEY` | 自动更新 | 当前 `uploadUpdaterJson: false`，未启用自动更新 |

## 二、发布一次

```bash
# 1. 确认本地干净、测试全绿
git status --short
pnpm typecheck && pnpm test
cargo test --manifest-path src-tauri/Cargo.toml

# 2. 打标签并推送（会自动把上海时间版本号写进 package.json 与 tauri.conf.json）
make push-tag
```

`make push-tag` 做的事：`update-version` → `package.json` 与 `src-tauri/tauri.conf.json` 的
`version` 改成 `YY.<MMDD>.<HHMM>`（例：`26.1110.2308`）→ `git tag v26.1110.2308` → push tag。

推送后工作流依次执行：

1. `prepare`：按 tag 创建（或复用）GitHub Release
2. `test`：`pnpm typecheck` + `pnpm test` + `cargo test`（**任一失败则不构建**）
3. `build`：4 个矩阵并行出包并上传到该 Release
   - macOS arm64 DMG（`macos-15`）
   - macOS x64 DMG（`macos-15-intel`）
   - Windows x64 NSIS（`windows-latest`）
   - Linux x64 AppImage + DEB（`ubuntu-22.04`）

工作流里的 `Sync app version from release tag` 会再次用 tag 覆盖版本号，保证包内版本与 tag 严格一致。

## 三、重新构建已有标签

```bash
gh workflow run release.yml -f tag=v26.1110.2308
```

或 GitHub 页面 → Actions → Release → Run workflow，填入已存在的 tag。

## 四、发布前自检清单

- [ ] CI（`.github/workflows/ci.yml`）在 `main` 上是绿的
- [ ] tag 格式为 `v<数字>.<数字>.<数字>`（`release.yml` 里的正则是 `^v\d+\.\d+\.\d+$`）
- [ ] `git status` 干净，没有把 `.devhome/`、`.appdata/` 等本地缓存目录提交进去（已在 `.gitignore`）
- [ ] Release 说明里的 AGPL 与非官方分支声明仍在（`prepare` 作业里生成）
- [ ] 若本次带了数据库结构变更：确认 `db/mod.rs` 的 `SCHEMA_VERSION` 已递增，老用户升级会走 `apply_migrations`
- [ ] 构建后抽查 deb 元数据：`dpkg-deb -f <包>.deb Package` 必须输出 ASCII 包名，`dpkg-deb -I <包>.deb` 的
      `Maintainer` / `Description` 应是本分支信息

## 五、已知风险

1. **`productName` 必须是 ASCII**（现为 `OneMail Wanglai`）：Debian 包名只允许小写字母、数字与 `-+._`，
   早前的 `OneMail 往来` 会让 `dpkg` 直接拒收（`无效的软件包名 ... 不允许出现字符`）。
   中文名只出现在应用内部（托盘 tooltip、通知署名、界面文案 `common.oneMail`），不参与打包。
   若想让桌面图标也显示中文，可用 Tauri 的 `bundle.linux.deb.desktopTemplate` 单独覆盖 `.desktop` 的 `Name=`。
2. **未签名安装包**会被系统提示；开源项目可以直接说明「首次运行请右键打开」。
3. **自动更新未启用**：`release.yml` 里 `uploadUpdaterJson: false`，且 `tauri.conf.json` 没有 updater 配置。
   要启用需生成签名密钥对并补 `plugins.updater` 配置。
4. **数据库结构与本分支绑定**：本分支会持续加表/加列，不建议让用户在上游版本与本分支之间混用同一个
   `.onemail` 备份（备份元数据只校验 `format_version`，不校验分支）。
