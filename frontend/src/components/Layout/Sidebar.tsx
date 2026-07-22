/*
 * @Author: 1orz cloudorzi@gmail.com
 * @Date: 2025-12-10 09:19:05
 * @LastEditors: 1orz cloudorzi@gmail.com
 * @LastEditTime: 2026-04-18 00:00:00
 * @FilePath: /udx710-backend/frontend/src/components/Layout/Sidebar.tsx
 * @Description:
 *
 * Copyright (c) 2025 by 1orz, All Rights Reserved.
 */
import { useNavigate, useLocation } from 'react-router-dom'
import {
  Drawer,
  List,
  ListItem,
  ListItemButton,
  ListItemIcon,
  ListItemText,
  ListSubheader,
  Toolbar,
  Divider,
  Box,
  Typography,
  Link,
  IconButton,
  Stack,
  Tooltip,
} from '@mui/material'
import {
  MenuOpen as MenuOpenIcon,
  MoreVert as MoreVertIcon,
  Refresh as RefreshIcon,
} from '@mui/icons-material'
import { appPages, githubLink, navGroups } from '../../navigation'

interface SidebarProps {
  drawerWidth: number
  mobileOpen: boolean
  desktopOpen: boolean
  onClose: () => void
  isMobile: boolean
  onMenuClick: () => void
  onRefreshClick: () => void
  onMenuOptionsClick: (event: React.MouseEvent<HTMLElement>) => void
}

export default function Sidebar({
  drawerWidth,
  mobileOpen,
  desktopOpen,
  onClose,
  isMobile,
  onMenuClick,
  onRefreshClick,
  onMenuOptionsClick,
}: SidebarProps) {
  const navigate = useNavigate()
  const location = useLocation()
  const GithubIcon = githubLink.icon

  const handleNavigation = (path: string): void => {
    void navigate(path)
    if (isMobile) {
      onClose()
    }
  }

  const drawer = (
    <Box sx={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <Toolbar sx={{ px: 2, justifyContent: 'space-between' }}>
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, minWidth: 0 }}>
          <Typography variant="h6" noWrap component="div" fontWeight={600}>
            UDX710
          </Typography>
        </Box>
        <Tooltip title={isMobile ? '关闭侧边栏' : '折叠侧边栏'}>
          <IconButton edge="end" size="small" onClick={onMenuClick}>
            <MenuOpenIcon />
          </IconButton>
        </Tooltip>
      </Toolbar>
      <Divider />
      <List sx={{ flexGrow: 1, py: 1 }}>
        {navGroups.map((group) => {
          const groupPages = appPages.filter((page) => page.group === group.id)

          return (
            <Box key={group.id} component="li" sx={{ listStyle: 'none' }}>
              <ListSubheader
                component="div"
                disableSticky
                sx={{
                  lineHeight: 2,
                  fontSize: '0.7rem',
                  fontWeight: 700,
                  letterSpacing: 0,
                  color: 'text.disabled',
                  bgcolor: 'transparent',
                  px: 2,
                  pt: 1,
                }}
              >
                {group.label}
              </ListSubheader>
              {groupPages.map((item) => {
                const IconComponent = item.icon

                return (
                  <ListItem key={item.path} disablePadding>
                    <ListItemButton
                      selected={location.pathname === item.path}
                      onClick={() => handleNavigation(item.path)}
                      sx={{ mx: 1, borderRadius: 1 }}
                    >
                      <ListItemIcon sx={{ minWidth: 40 }}>
                        <IconComponent />
                      </ListItemIcon>
                      <ListItemText primary={item.label} />
                    </ListItemButton>
                  </ListItem>
                )
              })}
            </Box>
          )
        })}
      </List>
      <Box sx={{ p: 2, borderTop: 1, borderColor: 'divider' }}>
        <Stack direction="row" spacing={1} sx={{ mb: 1.5 }}>
          <Tooltip title="刷新当前数据">
            <IconButton size="small" onClick={onRefreshClick}>
              <RefreshIcon fontSize="small" />
            </IconButton>
          </Tooltip>
          <Tooltip title="更多选项">
            <IconButton size="small" onClick={onMenuOptionsClick}>
              <MoreVertIcon fontSize="small" />
            </IconButton>
          </Tooltip>
        </Stack>
        <Link
          href={githubLink.href}
          target="_blank"
          rel="noopener noreferrer"
          sx={{
            display: 'flex',
            alignItems: 'center',
            gap: 0.5,
            color: 'text.secondary',
            textDecoration: 'none',
            fontSize: '0.75rem',
            '&:hover': {
              color: 'primary.main',
            },
          }}
        >
          <GithubIcon sx={{ fontSize: 16 }} />
          <Typography variant="caption" color="inherit">
            {githubLink.label}
          </Typography>
        </Link>
        <Typography variant="caption" color="text.disabled" sx={{ display: 'block', mt: 0.5 }}>
          v{__APP_VERSION__} ({__GIT_BRANCH__}/{__GIT_COMMIT__})
        </Typography>
        <Typography variant="caption" color="text.disabled" sx={{ display: 'block', mt: 0.5 }}>
          Copyright 2026
        </Typography>
      </Box>
    </Box>
  )

  return (
    <Box
      component="nav"
      sx={{
        width: { xs: 0, sm: desktopOpen ? drawerWidth : 0 },
        flexShrink: { sm: 0 },
        transition: 'width 0.3s',
      }}
    >
      <Drawer
        variant="temporary"
        open={mobileOpen}
        onClose={onClose}
        ModalProps={{
          keepMounted: true,
        }}
        sx={{
          display: { xs: 'block', sm: 'none' },
          '& .MuiDrawer-paper': {
            boxSizing: 'border-box',
            width: drawerWidth,
          },
        }}
      >
        {drawer}
      </Drawer>

      <Drawer
        variant="persistent"
        open={desktopOpen}
        sx={{
          display: { xs: 'none', sm: 'block' },
          '& .MuiDrawer-paper': {
            boxSizing: 'border-box',
            width: drawerWidth,
            transition: 'transform 0.3s',
          },
        }}
      >
        {drawer}
      </Drawer>
    </Box>
  )
}
