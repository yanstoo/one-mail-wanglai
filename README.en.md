# OneMail Wanglai

<p align="left">
  <img src="https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white" alt="Tauri 2" />
  <img src="https://img.shields.io/badge/Rust-stable-000000?logo=rust&logoColor=white" alt="Rust stable" />
  <img src="https://img.shields.io/badge/React-19-282C34?logo=react&logoColor=61DAFB" alt="React 19" />
  <img src="https://img.shields.io/badge/TypeScript-5.9-3178C6?logo=typescript&logoColor=white" alt="TypeScript 5.9" />
  <img src="https://img.shields.io/badge/Vite-7-646CFF?logo=vite&logoColor=white" alt="Vite 7" />
  <img src="https://img.shields.io/badge/Tailwind_CSS-4-06B6D4?logo=tailwindcss&logoColor=white" alt="Tailwind CSS 4" />
  <img src="https://img.shields.io/badge/SQLite-local-003B57?logo=sqlite&logoColor=white" alt="SQLite" />
  <img src="https://img.shields.io/badge/pnpm-10.34-F69220?logo=pnpm&logoColor=white" alt="pnpm 10.34" />
</p>

> **This repository is an unofficial fork of OneMail.**
>
> - Upstream: [zhihui-hu/one-mail](https://github.com/zhihui-hu/one-mail) (Copyright (C) Huzhihui)
> - This fork: OneMail Wanglai — reorganizes the mailbox into a per-correspondent timeline; see [FORK.md](./FORK.md) for the diff
> - License: [AGPL-3.0-only](./LICENSE) (same as upstream); attribution in [NOTICE](./NOTICE)
> - Maintained independently. Please report issues here, not to the upstream author.

OneMail is a local-first desktop mail client built with Tauri 2, Rust, React, and TypeScript. Mail sync, sending, OAuth, SQLite access, and the optional AI assistant run in Rust, while the interface uses React and Vite.

**Language**: [中文](./README.md) | English

**Upstream website / downloads**: [https://zhihui-hu.github.io/one-mail/](https://zhihui-hu.github.io/one-mail/) (upstream, not this fork)

## Features

### Correspondence view (core of this fork)

- 🧑‍🤝‍🧑 **Organised by people, not folders**: the left pane lists one row per correspondent ordered by the arrival time of the latest mail; the middle pane is a chat-style timeline; the right pane shows the message.
- 📐 **Who counts as a correspondent**: for received mail, only the **sender**; for mail you sent, the **to / cc / bcc** recipients. Co-recipients of a broadcast message are not treated as correspondents, and your own addresses are always excluded.
- 🗃️ **Normal / Junk groups**: junk has its own grey group; the junk folder is always synced and is detected even when the server only exposes a folder name (`Spam`, `垃圾邮件`, `廣告`).
- 🏷️ **Badges**: the account badge sits on the avatar's bottom-right corner (editable text, ≤10 chars, custom colour); the counterpart's registrable domain (`mail.google.com` → `google`) becomes an automatic badge on its own right-aligned row, coloured by first letter — the avatar uses the same palette.
- 📝 **Contact aliases**: rename a contact from the context menu; aliases survive a full index rebuild.

### Accounts and sync

- 📬 **Multiple accounts**: Gmail, Yahoo, Alibaba, QQ/Foxmail, Tencent Exmail, NetEase, Outlook, Sina, 139, iCloud, AOL, Yandex, Mail.ru and custom IMAP/SMTP.
- 🔑 **Verified on creation**: a new account performs a real IMAP login first; on failure nothing is stored and the server error is translated into a readable reason (bad password, unknown user, DNS failure, refused port, TLS mismatch, IMAP disabled).
- 🔄 **Sync**: folder-incremental IMAP sync, delta channels for Gmail/Graph; the folder list is refreshed before each sync and both `INBOX` and the junk folder are always enabled.
- 📶 **Progress**: the status bar shows "syncing x/y · folder".

### Reading mail

- 📨 **Bodies**: loaded on demand and prefetched after each sync for mail from the last 30 days up to 5 MB (max 40 per round).
- 🧭 **Moved mail still opens**: when the body is missing the message is relocated by `Message-ID` across every server folder and its folder/UID is updated; as a last resort the raw message is fetched in chunks.
- 🛡️ **Sanitised HTML**: remote images and external resources are blocked by default.
- 📎 **Attachments**: inline preview for images, save to `~/Downloads` (auto-numbered), everything else opens with the system handler; missing attachment records are re-parsed on demand.
- 🌐 **Paragraph translation**: translations appear under each paragraph; backends: Google, LibreTranslate, Azure, DeepL.
- ✅ **Read state**: marked locally first, then pushed as `\Seen` on a best-effort basis; a failed push never rolls back the local state.

### Desktop and packaging

- 🪟 **Custom title bar**: borderless on Linux/Windows with controls at the top right in minimize → maximize → close order; double-clicking the bar toggles maximise. macOS keeps the native traffic lights.
- ⚙️ **Settings**: an in-window modal (not a separate window) with general, **account management**, AI, translation, backup and about sections.
- 🔐 **Encrypted credentials**: AES-256-GCM with the key stored in the app data directory.
- 📦 **Linux deb**: package `one-mail-wanglai`, binary `onemail-wanglai`; installs side by side with upstream `one-mail` without file conflicts.

## Interface

Three panes built around people rather than folders:

1. **Left · correspondents**: one row per address, newest correspondence first; group tabs for Normal / Junk with unread badges and an "all mailboxes" filter.
2. **Middle · timeline**: every message with that person grouped by day; unread bubbles are bold with a dot; the footer button writes a **new mail to them**.
3. **Right · message**: toolbar with reply, reply-all, translate, forward, mark as junk and delete; then sender, recipients and CC (long lists wrap), body and attachments.

Interaction notes:

- **Replies only start from the bubble context menu** (with a quote); the footer is a fresh message.
- Opening an unread mail marks it read and refreshes badges and bubble styling.
- Switching correspondents clears the right pane so no stale message can leak through.
- If the contact only appears in the To / CC list, the reader explains why the message is filed under that person.
- Attachments: images preview inline, "Download" saves to `~/Downloads`, PDFs and others open in the system handler.

## Account Authorization

- **Gmail**: uses Google OAuth in the system browser by default, with an app-password option under Advanced. Google OAuth requires `ONEMAIL_GOOGLE_CLIENT_ID` at runtime.
- **Outlook / Hotmail**: uses Microsoft OAuth in the system browser. `ONEMAIL_MICROSOFT_CLIENT_ID` can override the built-in desktop client ID.
- **Other built-in providers**: enter a password, app password, authorization code, or client-specific password as required by the provider.
- **Custom IMAP**: enter the email address, credentials, IMAP server, port, and security mode. SMTP can also be configured when sending is needed.
- **Folder discovery**: password- and authorization-code accounts can test IMAP and choose folders before saving. OAuth accounts currently use the provider's default sync path.

---

## AI Assistant

The AI assistant is disabled by default. Configure it under **Settings → AI**:

- It uses an OpenAI-compatible Chat Completions endpoint. Enter a base URL and model, then verify the connection before saving.
- Remote services must use HTTPS and require an API key. Local `localhost` or loopback services may use HTTP and do not use an API key.
- The API key is stored in the operating system credential store, not in SQLite or `.onemail` backups.
- The selected email's subject, sender, time, and plain-text content are sent to the configured service only when the user submits a request.
- The assistant is read-only: it can analyze content and generate drafts, but it has no tools and cannot act on mail directly.

---

## Local Development

### Requirements

- Node.js 22 or newer
- pnpm 10.34.1
- Rust stable toolchain
- The platform-specific [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/)
- macOS, Windows, or Linux desktop environment

### Install Dependencies

```bash
pnpm install
```

### Start Desktop Development

```bash
pnpm tauri:dev
# or
make dev
```

Tauri starts the Rust desktop process and the Vite development server together. Run `pnpm dev` only when you need the frontend server by itself.

### Checks and Frontend Build

```bash
pnpm typecheck
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

`pnpm build` generates frontend assets only; it does not produce a desktop installer.

### Package and Release

```bash
pnpm tauri:build
```

You can also run `make deploy`. It generates a Shanghai-time version, updates `package.json`, and then builds production bundles. Pushing a `v*` tag triggers GitHub Actions builds for macOS arm64/x64, Windows x64, and Linux x64.

---

## Usage

1. **Add a mailbox**: "+" in the header → pick a provider or custom IMAP → fill in host/port/password. The login is verified before saving, so a wrong account never lands in the database.
2. **Sync**: sync starts automatically after saving; the header also has a manual sync button. The junk folder is synced by default and can be paused per account in Settings → Accounts.
3. **Find people**: the left pane is ordered by arrival time; the group tab switches to Junk; the dropdown filters by mailbox.
4. **Read**: click a bubble in the middle pane; unread messages become read automatically.
5. **Reply**: right-click a bubble → Reply / Reply all; the reader switches to a composer with the quote. The footer button writes a new mail without a quote.
6. **Translate**: click Translate; translations appear under each paragraph; click again to collapse.
7. **Attachments**: use Preview / Download below the body; downloads land in `~/Downloads`.
8. **Manage accounts**: Settings → Accounts to edit the alias, badge and colour, pause sync or remove an account.
9. **Backup**: Settings → Backup exports an `.onemail` snapshot and can restore it.

## Data and Security

- The SQLite database is stored at `OneMail/onemail.sqlite` under Tauri's application data directory.
- Mailbox passwords, authorization codes, and OAuth tokens are encrypted with an AES-256-GCM key derived from the local database key.
- The AI API key stays in the operating system credential store. Conversation and selected-email content leave the device only after a user request to the configured AI service.
- A `.onemail` file is a SQLite snapshot containing all local database data and recovery-key metadata. Treat it as a sensitive file.
- Import validates SQLite integrity, required tables, the format version, and backup metadata. Legacy `.sql` remains an import-only compatibility format.
- HTML mail is sanitized. Remote images and external resources are blocked by default to reduce privacy leakage.
- Received attachments are currently stored as metadata only and are not written to local files automatically.

---

## Current Limitations

- **Delete is local only** — messages are not removed or moved on the server.
- **Marking junk is local only** — no IMAP MOVE yet.
- Body prefetch covers the last 30 days, ≤5 MB, max 40 per round; older or larger mail is fetched on demand.
- PDFs are not rendered inline (WebKitGTK has no built-in PDF plugin); Preview opens the system viewer. Inline PDF would need pdf.js.
- IMAP syncs at most the most recent 200 messages per enabled folder; sync interval, cache window and autostart are not wired to system behaviour yet.
- Remote backup (WebDAV / S3) and auto-update feeds are not configured.
- OAuth, IMAP, SMTP and AI availability depends on real accounts and services — verify before production use.

## Project Structure

```text
src/                              # Frontend (React 19 + TS)
├── app/                          # Router: / → correspondence view
├── components/
│   ├── account/                  # Add-account dialog and form
│   ├── settings/                 # Settings modal (incl. accounts / translation)
│   ├── mail/                     # Composer and message rendering
│   ├── window-controls.tsx       # Custom window buttons for Linux/Windows
│   └── context-menu.tsx          # Context menu (alias, reply)
├── features/correspondents/      # Contact list, timeline, message reader
├── lib/                          # Tauri API adapter, i18n, query client
├── pages/                        # Shared outbox / account pages
└── shared/                       # Shared types and provider metadata

src-tauri/src/                    # Backend (Rust)
├── commands/
│   ├── data/                     # Accounts, messages, drafts, correspondents, translate
│   ├── backup/                   # .onemail import/export
│   └── system.rs                 # Windows, logging, paths, settings window
├── db/
│   ├── schema.sql                # Schema (incl. the three correspondence tables)
│   ├── correspondents.rs         # Index rebuild / incremental ingest / ordering
│   └── mod.rs                    # Connections and SCHEMA_VERSION migrations
├── mail/
│   ├── sync/                     # IMAP / Gmail / Graph sync
│   ├── body/                     # Body fetch, attachments, prefetch
│   ├── transport/                # IMAP connection and folder discovery
│   └── smtp.rs                   # Sending
├── translate/                    # Translation backends
├── ai/                           # Read-only AI assistant
└── lib.rs                        # Commands, tray, single instance, window events
```

---

## Tech Stack

- [Tauri 2](https://tauri.app/) - cross-platform desktop application framework
- [Rust](https://www.rust-lang.org/) - mail, OAuth, backup, and local-data core
- [React](https://react.dev/) - renderer UI
- [React Router](https://reactrouter.com/) - frontend routing
- [Vite](https://vite.dev/) - frontend development and build tooling
- [TypeScript](https://www.typescriptlang.org/) - type system
- [Tailwind CSS](https://tailwindcss.com/) - utility-first styling
- [shadcn/ui](https://ui.shadcn.com/) - UI components
- [Lucide React](https://lucide.dev/) - icon library
- [SQLite](https://www.sqlite.org/) / [rusqlite](https://github.com/rusqlite/rusqlite) - local storage and native backup
- [async-imap](https://github.com/async-email/async-imap) / [lettre](https://lettre.rs/) - IMAP sync and SMTP sending

---

## Contributing

Issues and pull requests are welcome. Before submitting, please run:

```bash
pnpm typecheck
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
```

## License

OneMail is licensed under the [GNU Affero General Public License v3.0](https://www.gnu.org/licenses/agpl-3.0.html) (AGPL-3.0-only).

You may use, copy, modify, and distribute this project under AGPL v3.0. If you provide a modified version over a network, you must also provide the corresponding source code as required by the AGPL.

## Credits

- [Tauri](https://tauri.app/) - desktop runtime
- [Rust](https://www.rust-lang.org/) - native application core
- [React](https://react.dev/) - interface framework
- [Vite](https://vite.dev/) - development and build tooling
- [shadcn/ui](https://ui.shadcn.com/) - UI component library
- [Lucide](https://lucide.dev/) - icons
- [SQLite](https://www.sqlite.org/) - local database

---

**Note**: Gmail and Outlook prefer OAuth in the system browser. Other providers usually require IMAP/SMTP to be enabled and an app password, authorization code, or client-specific password.
