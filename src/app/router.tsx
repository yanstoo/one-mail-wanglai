import { createHashRouter, Navigate } from 'react-router'

import { AddAccountPage } from '@renderer/pages/accounts/new'
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
