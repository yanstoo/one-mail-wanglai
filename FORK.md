# 本分支说明（FORK）

## 来源

- 上游项目：<https://github.com/zhihui-hu/one-mail>（Copyright (C) Huzhihui）
- 本分支仓库：<https://github.com/yanstoo/one-mail>
- 分叉起点：`main` @ `d1be24d`（2026-09-24）
- 授权：AGPL-3.0-only，与上游一致（全文见 [LICENSE](./LICENSE)，版权归属见 [NOTICE](./NOTICE)）

## 本分支要做什么

把「按邮箱分组的收件箱」改造成「按来往联系人归并的往来时间线」：

- **左栏**：一个来往邮箱地址一行，按该地址全部邮件的最新时间倒序；行右下角用短徽标标出这封邮件落在我的哪个邮箱（最多 2 个，溢出显示 `+N`，hover 展示全部）。
- **左栏分组**：普通 / 垃圾两个分组 tab，各带未读角标（普通红、垃圾灰）。分组只影响联系人归属、排序键与角标取数，不影响内容。
- **中栏**：与某个联系人的全部往来（不按我的邮箱分组、全部平铺），聊天气泡布局——对方在左、我在右，只显示标题，点击在右栏加载正文；垃圾邮件用灰标区分。
- **右栏**：正文阅读器；写信界面内嵌在右栏（上游是右下角浮层），右上「写邮件」与中栏「回复」共用同一处。
- **数据层**：新增跨全部邮箱账号的联系人索引表（`onemail_correspondents` 等），替代原有的运行时聚合查询。

## 与上游的边界（重要）

为了能持续吃到上游的协议修复，本分支刻意把改动收在「外壳」层：

| 层 | 目录 | 策略 |
|---|---|---|
| 内核（尽量零改动） | `src-tauri/src/mail/`、`src-tauri/src/oauth/`、`src-tauri/src/db/schema.sql` | 只在必要时改动，改动处加 `FORK:` 注释 |
| 外壳（本分支主战场） | `src-tauri/src/commands/data/`、`src/features/`、`src/components/` | 自由改 |

上游同步方式：

```bash
git remote add upstream git@github.com:zhihui-hu/one-mail.git
git fetch upstream
git log --oneline main..upstream/main -- src-tauri/src/mail src-tauri/src/oauth   # 只看内核修复
git cherry-pick <commit>                                                          # 按需挑，不整体 merge
```

## 命名与标识

- 产品名（打包/桌面条目，**必须是 ASCII**）：`OneMail Wanglai`（开发态 `OneMail Wanglai Dev`）
  —— 原因：Debian 包名只允许小写 ASCII，中文会让 `dpkg` 直接拒收
- 界面显示名（托盘、通知、关于页、i18n）：**OneMail 往来**
- identifier：`com.huzhihui.onemail.wanglai`（开发态 `.dev`）
  沿用上游前缀 + `wanglai` 子后缀，目的是避免与上游安装包**共享数据目录与单实例锁**——同 identifier 会导致两个版本读写同一个 SQLite、互相抢窗口。
- 以下标识刻意**保持不变**，以免引入迁移成本：Cargo/npm 包名 `onemail`、lib 名 `onemail_lib`、数据表前缀 `onemail_`、备份扩展名 `.onemail`、环境变量前缀 `ONEMAIL_*`。

## 已知差异与风险

- 本分支的数据库版本会逐渐与新上游不同，两边备份（`.onemail`）不建议互换导入。
- 上游官网与下载页（`docs/index.html`）仍是上游内容，本分支尚未替换。
