import type { ConversationListQuery, ConversationLocation, ConversationMessage, ConversationMessagesQuery, ConversationSearchResult, ConversationSummary } from '../conversations'
import type { AccountCreateInput, AccountCreatedEvent, AccountUpdateInput, ImapFolder, ImapFolderDiscoveryInput, MailAccount, SyncMode } from './accounts'
import type { CorrespondentGroupCounts, CorrespondentListQuery, CorrespondentListResult, CorrespondentThreadResult } from './correspondents'
import type { AccountMailboxStats, ComposeDraft, ForwardDraftInput, MailAttachmentInput, MailMessageBodyLoadResult, MailMessageDetail, MailSendInput, MailSendResult, MessageReadStateUpdate, OutboxListQuery, OutboxMessage, ReplyDraftInput } from './messages'
import type { AccountSyncRunResult, MailboxChangedEvent, NewMailNotification, SyncAllRunResult, SyncProgressEvent } from './sync'
import type { AiChatInput, AiChatResult, AiSettings, AiSettingsInput, AppSettings, AppTheme, AppUpdateStatus, BackupImportProgress, BackupImportResult, BackupSyncDownloadResult, BackupSyncSettings, BackupSyncTestResult, BackupSyncTransferResult, SettingsUpdateInput, SystemInfo } from './settings'

export type OneMailApi = {
  conversations: {
    list: (query?: ConversationListQuery) => Promise<ConversationSummary[]>
    messages: (query: ConversationMessagesQuery) => Promise<ConversationMessage[]>
    findMessage: (messageId: number) => Promise<ConversationLocation | null>
    search: (keyword: string, limit: number, offset: number) => Promise<ConversationSearchResult[]>
  }
  correspondents: {
    list: (query: CorrespondentListQuery) => Promise<CorrespondentListResult>
    thread: (correspondentId: number, before?: string | null, limit?: number) => Promise<CorrespondentThreadResult>
    groupCounts: () => Promise<CorrespondentGroupCounts>
    /** 设置联系人备注名；传 null 清除 */
    setAlias: (correspondentId: number, alias: string | null) => Promise<{ correspondentId: number; alias: string | null }>
  }
  accounts: {
    list: () => Promise<MailAccount[]>
    create: (input: AccountCreateInput) => Promise<MailAccount>
    discoverFolders: (input: ImapFolderDiscoveryInput) => Promise<ImapFolder[]>
    onCreated: (callback: (event: AccountCreatedEvent) => void) => () => void
    openAddWindow: () => Promise<boolean>
    closeAddWindow: () => Promise<boolean>
    update: (input: AccountUpdateInput) => Promise<MailAccount>
    reauthorize: (accountId: number) => Promise<MailAccount>
    remove: (accountId: number) => Promise<boolean>
  }
  messages: {
    stats: () => Promise<AccountMailboxStats[]>
    get: (messageId: number) => Promise<MailMessageDetail | null>
    loadBody: (messageId: number) => Promise<MailMessageBodyLoadResult>
    saveAttachment: (messageId: number, index: number) => Promise<string>
    attachmentPayload: (
      messageId: number,
      index: number
    ) => Promise<{ filename: string; mimeType: string; sizeBytes: number; base64: string }>
    saveAttachmentAs: (messageId: number, index: number, targetPath: string) => Promise<string>
    downloadAttachment: (messageId: number, index: number) => Promise<string>
    setReadState: (messageId: number, isRead: boolean) => Promise<MessageReadStateUpdate>
    /** junk: true=标为垃圾，false=不是垃圾，null=跟随文件夹 */
    setJunkOverride: (messageId: number, junk: boolean | null) => Promise<{ messageId: number; junkOverride: boolean | null }>
    setDeleted: (messageId: number, deleted: boolean) => Promise<{ messageId: number; deleted: boolean }>
  }
  compose: {
    createReplyDraft: (input: ReplyDraftInput) => Promise<ComposeDraft>
    createForwardDraft: (input: ForwardDraftInput) => Promise<ComposeDraft>
    send: (input: MailSendInput) => Promise<MailSendResult>
    selectAttachments: () => Promise<MailAttachmentInput[]>
    listOutbox: (query?: OutboxListQuery) => Promise<OutboxMessage[]>
    saveDraft: (input: MailSendInput) => Promise<OutboxMessage>
    deleteDraft: (outboxId: number) => Promise<boolean>
    retry: (outboxId: number) => Promise<MailSendResult>
    deleteOutbox: (outboxId: number) => Promise<boolean>
    onSent: (callback: (result: MailSendResult) => void) => () => void
  }
  sync: {
    startAll: (mode?: SyncMode) => Promise<SyncAllRunResult>
    startAccount: (accountId: number, mode?: SyncMode) => Promise<AccountSyncRunResult>
    onProgress: (callback: (event: SyncProgressEvent) => void) => () => void
    onMailboxChanged: (callback: (event: MailboxChangedEvent) => void) => () => void
  }
  notifications: {
    onNewMail: (callback: (notification: NewMailNotification) => void) => () => void
  }
  settings: {
    get: () => Promise<AppSettings>
    update: (input: SettingsUpdateInput) => Promise<AppSettings>
    getBackupSync: () => Promise<BackupSyncSettings>
    updateBackupSync: (input: BackupSyncSettings) => Promise<BackupSyncSettings>
    testBackupSync: (input: BackupSyncSettings) => Promise<BackupSyncTestResult>
    uploadBackupSync: () => Promise<BackupSyncTransferResult>
    downloadBackupSync: (operationId?: string) => Promise<BackupSyncDownloadResult>
    importBackupFromRemote: (
      input: BackupSyncSettings,
      operationId?: string
    ) => Promise<BackupSyncDownloadResult>
    exportSql: () => Promise<string | null>
    importSql: (operationId?: string) => Promise<BackupImportResult>
    onBackupImportProgress: (callback: (progress: BackupImportProgress) => void) => () => void
  }
  translate: {
    text: (input: { text: string; backend?: TranslateBackend }) => Promise<{ text: string }>
    getSettings: () => Promise<TranslateSettings>
    saveSettings: (input: TranslateSettingsInput) => Promise<TranslateSettings>
  }
  ai: {
    getSettings: () => Promise<AiSettings>
    verifyAndSave: (input: AiSettingsInput) => Promise<AiSettings>
    clear: () => Promise<AiSettings>
    chat: (input: AiChatInput) => Promise<AiChatResult>
  }
  updates: {
    status: () => Promise<AppUpdateStatus>
    install: () => Promise<boolean>
    onStatus: (callback: (status: AppUpdateStatus) => void) => () => void
  }
  system: {
    getTheme: () => Promise<AppTheme | null>
    info: () => Promise<SystemInfo>
    memory: () => Promise<number>
    databaseSize: () => Promise<number>
    setTitleBarTheme: (theme: AppTheme) => Promise<boolean>
    revealLogs: () => Promise<boolean>
    openDatabaseDirectory: () => Promise<boolean>
    reloadClient: () => Promise<boolean>
    openDevtools: () => Promise<boolean>
    revealPath: (path: string) => Promise<boolean>
    openExternal: (url: string) => Promise<boolean>
    openPath: (path: string) => Promise<boolean>
    openSettings: (section?: string) => Promise<boolean>
    /** Google OAuth 客户端 ID：source 为 env / stored / none */
    getGoogleClientId: () => Promise<{ clientId: string | null; source: 'env' | 'stored' | 'none' }>
    /** 传 null 表示清除已保存的值 */
    setGoogleClientId: (clientId: string | null) => Promise<{ clientId: string | null; saved: boolean }>
  }
}

export type TranslateBackend = 'google' | 'libretranslate' | 'azure' | 'deepl'

export type TranslateSettings = {
  backend: TranslateBackend
  /** 目标语言；空 = 简体中文 */
  targetLang: string
  libretranslateUrl: string | null
  apiKeyConfigured: boolean
  apiRegion: string | null
  /** 当前用的是 Google 非官方端点 */
  googleUnofficial: boolean
}

export type TranslateSettingsInput = {
  backend?: TranslateBackend
  targetLang?: string
  libretranslateUrl?: string | null
  /** 传空串表示清除 */
  apiKey?: string | null
  apiRegion?: string | null
}
