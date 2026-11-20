# 本分支说明（FORK）

## 来源

- 上游：[zhihui-hu/one-mail](https://github.com/zhihui-hu/one-mail)（Copyright (C) Huzhihui）
- 本分支：**OneMail 往来**（仓库 `yanstoo/one-mail-wanglai`），以 AGPL-3.0-only 发布
- 定位：把上游「按文件夹/账号分组的收件箱」改成**按往来联系人组织的三栏视图**

## 一、界面与交互差异

| 项目 | 上游 | 本分支 |
| --- | --- | --- |
| 主入口 | 收件箱（文件夹维度） | **往来三栏**：联系人 → 时间线 → 正文 |
| 联系人聚合 | 无 | 一行一个邮箱地址，按「最近到达时间」倒序 |
| 中栏 | 邮件列表 | 聊天式时间线，按天分组；未读加粗 + 圆点 |
| 回复入口 | 底部工具栏 | **只从气泡右键进入**（带原文引用）；底部按钮是「写邮件给 TA」 |
| 窗口 | 系统装饰 | Linux/Windows 无系统装饰、自绘标题栏（按钮右上：最小化→最大化→关闭，双击切换最大化）；macOS 保留原生交通灯 |
| 设置 | 独立窗口 | **主窗口内模态弹窗**，新增「账号管理」分区 |
| 外部 AI | 无 | 内置 MCP 服务（本机 HTTP + 令牌，读邮件/写草稿/可选发送） |
| 徽标 | 无 | 账号短徽标（头像右下角，≤10 字、自定义颜色）+ 对方域名一级标签（自动配色，单独一行右对齐） |

## 二、数据模型差异

新增三张物化索引表（`src-tauri/src/db/schema.sql`）：

- `onemail_correspondents`：联系人（`address_norm` 唯一、`alias` 备注名、`last_at` 排序键、垃圾分组用 `junk_last_at`）
- `onemail_correspondent_accounts`：联系人 × 我的账号（徽标来源）
- `onemail_correspondent_messages`：联系人 × 邮件（`source` 区分 `mail` / `outbox` 两套 id 空间）

往来口径：

- **收到的邮件** → 往来人取 `from`
- **我发出的邮件**（Sent 文件夹 / 发件箱）→ 往来人取 `to` / `cc` / `bcc`
- 自己的地址永远排除；群发邮件的共同收件人**不算**往来人

## 三、同步与正文

- 同步前刷新服务器文件夹表，`INBOX` 与垃圾箱（`role=junk`）强制启用；文件夹识别兼容 `\Spam` 与常见中文名。
- 时间统一归一化为 ISO-8601 UTC：IMAP 优先取服务器 `INTERNALDATE`，缺失才用 `Date` 头。
- 正文双通道：
  1. **预取**：同步后补最近 30 天、≤5 MB、每轮最多 40 封（60 秒冷却去重）
  2. **兜底**：原文件夹取不到时，按 `Message-ID` 在全部服务器文件夹重新定位并回写位置；仍失败则 `HEADER` + 分片 `TEXT` 分段抓取
- 已读：本地先行，再尽力回写 `\Seen`；服务器不可达不回滚本地状态。

## 四、附件与翻译

- 附件：图片内嵌预览、下载到 `~/Downloads`（重名加序号）、其它类型交系统程序；老邮件缺附件记录时自动重新解析。
- 翻译：Google / LibreTranslate / Azure / DeepL 四后端；**逐段对照**（每段原文下方贴译文），按段缓存。

## 五、打包差异

- `productName` 必须是 ASCII：`OneMail Wanglai`（中文会被 `dpkg` 拒收）
- 二进制 `one-mail-wanglai`、identifier `com.huzhihui.onemail.wanglai`（开发态 `.wanglai.dev`）
- 与上游 `one-mail` 并存安装、零文件冲突、数据目录互不影响
- Ubuntu 24.04+ 的 GTK 依赖改名 `libgtk-3-0t64`，由 `scripts/patch-deb-depends.mjs` 在打包后修补控制文件

## 六、文档

- [README.md](./README.md)：当前功能与用法
- [docs/README.md](./docs/README.md)：文档索引
- [RELEASING.md](./RELEASING.md)：发版与打包

## 七、上游未改动的部分

同步/发送/OAuth/AI/备份等 Rust 核心能力沿用上游实现；本分支只在其上叠加往来索引、视图与打包配置。
