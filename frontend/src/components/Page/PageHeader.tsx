import type { ComponentType, ReactNode } from 'react'
import { Box, Stack, Typography, type SxProps, type Theme } from '@mui/material'
import type { SvgIconProps } from '@mui/material/SvgIcon'

interface PageHeaderProps {
  title: string
  subtitle?: string
  icon?: ComponentType<SvgIconProps>
  actions?: ReactNode
  sx?: SxProps<Theme>
}

export default function PageHeader({ title, subtitle, icon: Icon, actions, sx }: PageHeaderProps) {
  return (
    <Box
      sx={[
        {
          display: 'flex',
          alignItems: { xs: 'flex-start', sm: 'center' },
          justifyContent: 'space-between',
          gap: 2,
          minWidth: 0,
        },
        ...(Array.isArray(sx) ? sx : sx ? [sx] : []),
      ]}
    >
      <Box display="flex" alignItems="flex-start" gap={1.5} minWidth={0}>
        {Icon && (
          <Box
            sx={{
              display: { xs: 'none', sm: 'inline-flex' },
              mt: 0.05,
              color: 'primary.main',
            }}
          >
            <Icon sx={{ fontSize: { xs: '1.5rem', sm: '1.875rem' } }} />
          </Box>
        )}
        <Box minWidth={0}>
          <Typography
            component="h1"
            variant="h4"
            fontWeight={700}
            sx={{
              fontSize: { xs: '1.5rem', sm: '1.875rem' },
              lineHeight: 1.2,
              letterSpacing: 0,
            }}
          >
            {title}
          </Typography>
          {subtitle && (
            <Typography variant="body2" color="text.secondary" sx={{ mt: 0.75 }}>
              {subtitle}
            </Typography>
          )}
        </Box>
      </Box>

      {actions && (
        <Stack direction="row" spacing={1} useFlexGap flexWrap="wrap" justifyContent="flex-end">
          {actions}
        </Stack>
      )}
    </Box>
  )
}
