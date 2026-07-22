import type { ReactNode } from 'react'
import { Box, type SxProps, type Theme } from '@mui/material'
import { getPageById, type AppPageId } from '@/navigation'
import PageHeader from './PageHeader'

interface PageContainerProps {
  pageId: AppPageId
  children: ReactNode
  actions?: ReactNode
  sx?: SxProps<Theme>
}

export default function PageContainer({ pageId, children, actions, sx }: PageContainerProps) {
  const page = getPageById(pageId)

  return (
    <Box
      sx={[
        {
          display: 'flex',
          flexDirection: 'column',
          gap: { xs: 2, sm: 3 },
          minWidth: 0,
        },
        ...(Array.isArray(sx) ? sx : sx ? [sx] : []),
      ]}
    >
      <PageHeader title={page.title} subtitle={page.subtitle} icon={page.icon} actions={actions} />
      {children}
    </Box>
  )
}
