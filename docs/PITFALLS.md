# 踩坑记录

每条按「现象 → 根因 → 解决 → 预防」记录。新增条目追加到末尾，并在 `docs/README.md` 索引登记。

---

## 1. 联系人排序像按星期几排的

- **现象**：左栏顺序毫无规律，16 天前的排在最上面；时间显示也不对。
- **根因**：`onemail_mail_messages.received_at` 存的是 IMAP 的 **RFC 2822 原文**（`Wed, 28 Oct 2026 14:00:06 +0800 (CST)`），物化索引把它直接复制成 `last_at`，`ORDER BY last_at DESC` 于是比较字符串 → 按星期名首字母排序（`Wed` > `Thu` > `Tue` …）。
- **解决**：新增 `mail::sync::normalize_timestamp`，剥离 `(CST)` 注释后依次尝试 RFC2822 / RFC3339 / IMAP INTERNALDATE，统一输出 ISO-8601 UTC 毫秒；写入时归一化；`SCHEMA_VERSION` 4 迁移批量修复存量数据并重建索引。
- **预防**：任何参与排序/比较的时间列必须是 ISO-8601；新增时间字段先在迁移里归一化。

## 2. 改了算法，老库却不生效

- **现象**：改了索引生成逻辑，重装后行为没变。
- **根因**：`last_at` 等是**物化值**，`apply_migrations` 只看 `SCHEMA_VERSION`；版本没升就不会重建，老库一直用旧值。
- **解决**：凡是改变索引生成逻辑，都必须升 `SCHEMA_VERSION` 并在迁移里 `rebuild_all`。
- **预防**：改 `ingest_*` / `refresh_aggregates` 的语义时，在 commit 信息里显式写「需要迁移 vN」。

## 3. Tauri 的错误被前端吞掉，只显示兜底文案

- **现象**：点开邮件提示「标记已读失败」，添加账号只显示「保存失败」。
- **根因**：`invoke` 失败时 Rust 侧返回的是**字符串**，前端写 `cause instanceof Error ? cause.message : '兜底'` 永远走兜底分支。
- **解决**：错误处理统一按「字符串优先」编写；后端把失败原因写入客户端日志（`WARN Mark read failed …`）。
- **预防**：新增 `catch` 时先判断 `typeof cause === 'string'`。

## 4. 右栏显示的不是当前联系人的邮件

- **现象**：左栏选中 A，右栏正文却是 B 的邮件。
- **根因**：两件事叠加——① 切换联系人时没有清空 `selectedMessage/detail`；② 往来口径把「收到邮件的共同收件人」也算成了往来人，于是群发通知里同列的地址各成一个联系人，点开显示同一封邮件。
- **解决**：`selectedId` 变化立即重置右栏；口径改为「收到的取发件人、发出的取收件人/抄送」（v6 迁移重建）。
- **预防**：详情类状态以「当前选中 id」为键重建，不要跨选择复用。

## 5. 正文永远显示「（正文尚未缓存）」

- **现象**：明明预取成功，右栏仍是占位；错误信息为「读取邮件所属账号失败：Query returned no rows」。
- **根因**：三处叠加——① 前端调用 `loadBody` 却丢弃返回值，随后只取 `messages.get`（摘要，无 body）；② 解析失败时写入了一行**全 NULL** 的 `onemail_message_bodies`，让后续「已有缓存」判断永远命中；③ 跨文件夹定位成功后，回写位置时把 `RFC822 Message-ID` 当成数字 `message_id` 去查，报错并把正文一起丢弃。
- **解决**：并行取摘要与正文并合并；全空正文行视为未缓存并删除；解析为空不落库；回写统一按 `rfc822_message_id` 定位。
- **预防**：缓存判定必须校验「内容非空」；任何回写前先确认参数语义（数字 id ≠ Message-ID）。

## 6. 垃圾邮件一封都没同步

- **现象**：垃圾分组永远是空的。
- **根因**：① `folder_role` 只认 `\Junk` 属性，很多服务器（QQ/163/263）不给该属性；② 老账号的 `onemail_mail_folders` 只有 `INBOX` 一行，从未做过文件夹发现，而同步只读 `sync_enabled=1` 的文件夹。
- **解决**：`folder_role` 增加 `\Spam` 与名称兜底（`spam` / `垃圾` / `廣告`）；同步前用 `LIST` 刷新文件夹表，`inbox` 与 `junk` 强制启用同步。
- **预防**：依赖「服务器上有某类文件夹」的功能，必须自己确保发现过该文件夹。

## 7. 添加账号"成功"了但完全不能用

- **现象**：提示添加成功，实际一封邮件都同步不下来。
- **根因**：`accounts_create` 直接把记录插库并返回成功，从未验证 IMAP 登录。
- **解决**：入库前 `verify_imap_login`（登录 + 登出），失败直接返回「账号验证失败，未添加：…」。
- **预防**：任何外部依赖的写操作，先验证再落库。

## 8. 加了新功能，打包却仍是旧行为

- **现象**：`make deb` 之后安装，改动没生效。
- **根因**：`patch-deb-depends.mjs` 用**文件名字典序**取「最后一个 deb」，而版本号是 `YY.MMDD.HHMM`，`26.806.2208` 字符串上大于 `26.1112.2003`，于是修补了旧包；同时 `bundle/deb` 里堆着多版本。
- **解决**：改为按 **mtime** 取最新；`make deb` 先 `rm -rf bundle/deb`。
- **预防**：构建产物选择依据用时间而非名称；打包前清目录。

## 9. deb 被 dpkg 拒收 / Ubuntu 24.04 装不上

- **现象**：`dpkg: error: parsing file ... package name has characters that aren't lowercase`；或 24.04 上依赖无法满足。
- **根因**：`productName` 含中文（`OneMail 往来`）会被 Debian 拿来派生包名；`libgtk-3-0` 在 24.04+ 改名为 `libgtk-3-0t64`，而 Tauri 的 `depends` 是**追加**不是覆盖。
- **解决**：`productName` 改 ASCII `OneMail Wanglai`；打包后用脚本重写 `Depends` 为 `libgtk-3-0t64 | libgtk-3-0`。
- **预防**：Debian 相关标识一律 ASCII；跨发行版依赖用 `|` 备选。

## 10. 桌面环境下启动异常（沙箱/无头环境）

- **现象**：`pnpm --version` 报 `unable to open database file`；启动 panic `tray icon error: 只读文件系统`。
- **根因**：`HOME` 未重定向导致 pnpm 无法写库；`XDG_RUNTIME_DIR` 不可写时托盘图标创建失败。
- **解决**：开发用 `.devhome/env.sh` 重定向 `HOME`/`CARGO_HOME`/`XDG_RUNTIME_DIR`，并设置 `WEBKIT_DISABLE_COMPOSITING_MODE=1`。
- **预防**：无头/沙箱环境跑桌面应用，先把这三个变量和托盘目录准备好。
