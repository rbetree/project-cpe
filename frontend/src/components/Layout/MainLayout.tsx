/*
 * @Author: 1orz cloudorzi@gmail.com
 * @Date: 2025-11-22 10:30:41
 * @LastEditors: 1orz cloudorzi@gmail.com
 * @LastEditTime: 2026-04-18 20:15:00
 * @FilePath: /udx710-backend/frontend/src/components/Layout/MainLayout.tsx
 * @Description:
 *
 * Copyright (c) 2025 by 1orz, All Rights Reserved.
 */
import { useEffect, useRef, useState } from 'react'
import { Outlet } from 'react-router-dom'
import {
  Box,
  Divider,
  IconButton,
  ListItemIcon,
  ListItemText,
  Menu,
  MenuItem,
  type Theme,
  useMediaQuery,
  useTheme,
} from '@mui/material'
import {
  Brightness4 as DarkModeIcon,
  Brightness7 as LightModeIcon,
  Menu as MenuIcon,
  Speed as SpeedIcon,
} from '@mui/icons-material'

import { api } from '../../api'
import { RefreshContext } from '../../contexts/RefreshContext'
import { useTheme as useAppTheme } from '../../contexts/ThemeContext'
import { usePageVisibility } from '../../hooks/useAdaptivePolling'
import Sidebar from './Sidebar'

const DRAWER_WIDTH = 240
const DEFAULT_REFRESH_INTERVAL = 5000
// Keep the heartbeat comfortably below the backend timeout floor so 1s/3s polling
// does not drift onto the timeout boundary.
const HEARTBEAT_MIN_INTERVAL = 5000
const HIDDEN_HEARTBEAT_MIN_INTERVAL = 30000

function getHeartbeatInterval(refreshInterval: number, isPageVisible: boolean) {
  if (refreshInterval <= 0) {
    return isPageVisible ? 20000 : 60000
  }

  if (!isPageVisible) {
    return Math.max(refreshInterval * 12, HIDDEN_HEARTBEAT_MIN_INTERVAL)
  }

  return Math.max(refreshInterval * 3, HEARTBEAT_MIN_INTERVAL)
}

export default function MainLayout() {
  const theme = useTheme<Theme>()
  const { mode, toggleTheme } = useAppTheme()
  const isMobile = useMediaQuery(theme.breakpoints.down('sm'))
  const isPageVisible = usePageVisibility()
  const [mobileOpen, setMobileOpen] = useState(false)
  const [desktopOpen, setDesktopOpen] = useState(true)
  const [refreshInterval, setRefreshIntervalState] = useState(DEFAULT_REFRESH_INTERVAL)
  const [refreshKey, setRefreshKey] = useState(0)
  const refreshRequestIdRef = useRef(0)
  const [menuAnchor, setMenuAnchor] = useState<null | HTMLElement>(null)
  const [refreshAnchor, setRefreshAnchor] = useState<null | HTMLElement>(null)

  useEffect(() => {
    let cancelled = false

    const loadRefreshConfig = async () => {
      try {
        const response = await api.getRefreshConfig()
        if (
          !cancelled &&
          refreshRequestIdRef.current === 0 &&
          response.status === 'ok' &&
          response.data
        ) {
          setRefreshIntervalState(response.data.interval_ms)
        }
      } catch (error) {
        console.warn('加载刷新设置失败:', error)
      }
    }

    void loadRefreshConfig()

    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => {
    let cancelled = false

    const sendHeartbeat = async () => {
      try {
        await api.sendRefreshHeartbeat()
      } catch (error) {
        if (!cancelled) {
          console.warn('刷新心跳发送失败:', error)
        }
      }
    }

    void sendHeartbeat()

    const timer = window.setInterval(() => {
      void sendHeartbeat()
    }, getHeartbeatInterval(refreshInterval, isPageVisible))

    return () => {
      cancelled = true
      window.clearInterval(timer)
    }
  }, [isPageVisible, refreshInterval])

  const handleDrawerToggle = () => {
    if (isMobile) {
      setMobileOpen((current) => !current)
    } else {
      setDesktopOpen((current) => !current)
    }
  }

  const handleDrawerClose = () => {
    setMobileOpen(false)
  }

  const triggerRefresh = () => {
    setRefreshKey((current) => current + 1)
  }

  const handleMenuOpen = (event: React.MouseEvent<HTMLElement>) => {
    setMenuAnchor(event.currentTarget)
  }

  const handleMenuClose = () => {
    setMenuAnchor(null)
  }

  const handleRefreshMenuOpen = (event: React.MouseEvent<HTMLElement>) => {
    setRefreshAnchor(event.currentTarget)
  }

  const handleRefreshMenuClose = () => {
    setRefreshAnchor(null)
  }

  const handleThemeToggle = () => {
    toggleTheme()
    handleMenuClose()
  }

  const handleRefreshIntervalSelect = (interval: number) => {
    setRefreshInterval(interval)
    handleRefreshMenuClose()
    handleMenuClose()
  }

  const setRefreshInterval = (interval: number) => {
    const previousInterval = refreshInterval
    const requestId = refreshRequestIdRef.current + 1
    refreshRequestIdRef.current = requestId
    setRefreshIntervalState(interval)
    void api
      .setRefreshConfig(interval)
      .then((response) => {
        if (refreshRequestIdRef.current !== requestId) {
          return
        }

        if (response.status === 'ok' && response.data) {
          setRefreshIntervalState(response.data.interval_ms)
          return
        }

        setRefreshIntervalState(previousInterval)
        console.warn('保存刷新设置失败:', response.message)
      })
      .catch((error) => {
        if (refreshRequestIdRef.current === requestId) {
          setRefreshIntervalState(previousInterval)
        }
        console.warn('保存刷新设置失败:', error)
      })
  }

  return (
    <RefreshContext.Provider
      value={{ refreshInterval, setRefreshInterval, refreshKey, triggerRefresh }}
    >
      <Box sx={{ display: 'flex', minHeight: '100vh' }}>
        <Sidebar
          drawerWidth={DRAWER_WIDTH}
          mobileOpen={mobileOpen}
          desktopOpen={desktopOpen}
          onClose={handleDrawerClose}
          isMobile={isMobile}
          onMenuClick={handleDrawerToggle}
          onRefreshClick={triggerRefresh}
          onMenuOptionsClick={handleMenuOpen}
        />

        <IconButton
          aria-label="打开侧边栏"
          onClick={handleDrawerToggle}
          sx={{
            position: 'fixed',
            top: 16,
            left: 16,
            zIndex: (currentTheme) => currentTheme.zIndex.drawer + 1,
            display: {
              xs: mobileOpen ? 'none' : 'inline-flex',
              sm: desktopOpen ? 'none' : 'inline-flex',
            },
            bgcolor: 'background.paper',
            border: 1,
            borderColor: 'divider',
            boxShadow: 2,
            '&:hover': {
              bgcolor: 'action.hover',
            },
          }}
        >
          <MenuIcon />
        </IconButton>

        <Box
          component="main"
          sx={{
            flexGrow: 1,
            p: { xs: 2, sm: 3 },
            pt: { xs: 8, sm: desktopOpen ? 3 : 8 },
            width: {
              xs: '100%',
              sm: desktopOpen ? `calc(100% - ${DRAWER_WIDTH}px)` : '100%',
            },
            minHeight: '100vh',
            backgroundColor: 'background.default',
            transition: theme.transitions.create(['width', 'margin'], {
              easing: theme.transitions.easing.sharp,
              duration: theme.transitions.duration.leavingScreen,
            }),
          }}
        >
          <Outlet />
        </Box>

        <Menu
          anchorEl={menuAnchor}
          open={Boolean(menuAnchor)}
          onClose={handleMenuClose}
          anchorOrigin={{ vertical: 'bottom', horizontal: 'left' }}
          transformOrigin={{ vertical: 'top', horizontal: 'left' }}
        >
          <MenuItem onClick={handleThemeToggle}>
            <ListItemIcon>
              {mode === 'dark' ? <LightModeIcon fontSize="small" /> : <DarkModeIcon fontSize="small" />}
            </ListItemIcon>
            <ListItemText>{mode === 'dark' ? '浅色模式' : '深色模式'}</ListItemText>
          </MenuItem>
          <Divider />
          <MenuItem onClick={handleRefreshMenuOpen}>
            <ListItemIcon>
              <SpeedIcon fontSize="small" />
            </ListItemIcon>
            <ListItemText primary="刷新频率" secondary={refreshInterval === 0 ? '手动' : `${refreshInterval / 1000} 秒`} />
          </MenuItem>
        </Menu>

        <Menu
          anchorEl={refreshAnchor}
          open={Boolean(refreshAnchor)}
          onClose={handleRefreshMenuClose}
          anchorOrigin={{ vertical: 'top', horizontal: 'right' }}
          transformOrigin={{ vertical: 'top', horizontal: 'left' }}
        >
          <MenuItem selected={refreshInterval === 1000} onClick={() => handleRefreshIntervalSelect(1000)}>1 秒/次</MenuItem>
          <MenuItem selected={refreshInterval === 3000} onClick={() => handleRefreshIntervalSelect(3000)}>3 秒/次</MenuItem>
          <MenuItem selected={refreshInterval === 5000} onClick={() => handleRefreshIntervalSelect(5000)}>5 秒/次</MenuItem>
          <MenuItem selected={refreshInterval === 10000} onClick={() => handleRefreshIntervalSelect(10000)}>10 秒/次</MenuItem>
          <Divider />
          <MenuItem selected={refreshInterval === 0} onClick={() => handleRefreshIntervalSelect(0)}>手动刷新</MenuItem>
        </Menu>
      </Box>
    </RefreshContext.Provider>
  )
}
