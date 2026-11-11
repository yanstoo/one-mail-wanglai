import { createHashRouter, Navigate } from 'react-router'

import { AddAccountPage } from '@renderer/pages/accounts/new'
import { MailboxPage } from '@renderer/pages/mailbox'
import { CorrespondentsPage } from '@renderer/features/correspondents'
import { SettingsWindow } from '@renderer/components/settings/settings-window'
import { OriginalMessageWindow } from '@renderer/components/conversations/conversation-workspace/original-message-window'
import { RouteErrorPage } from '@renderer/components/app-error-page'

export const appRouter = createHashRouter([
  {
    errorElement: <RouteErrorPage />,
    children: [
      {
        path: '/',
        element: <CorrespondentsPage />
      },
      {
        // 旧版收件箱保留为「经典收件箱」，不再作为默认入口
        path: '/mailbox',
        element: <MailboxPage />
      },
      {
        path: '/correspondents',
        element: <Navigate to="/" replace />
      },
      {
        path: '/accounts/new',
        element: <AddAccountPage />
      },
      {
        path: '/settings',
        element: <SettingsWindow />
      },
      {
        path: '/original-message',
        element: <OriginalMessageWindow />
      },
      {
        path: '*',
        element: <Navigate to="/" replace />
      }
    ]
  }
])
