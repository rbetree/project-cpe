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
import { Box, type Theme, useMediaQuery, useTheme } from '@mui/material'

import { api } from '../../api'
import { RefreshContext } from '../../contexts/RefreshContext'
import { usePageVisibility } from '../../hooks/useAdaptivePolling'
import Sidebar from './Sidebar'
import TopBar from './TopBar'

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
  const isMobile = useMediaQuery(theme.breakpoints.down('sm'))
  const isPageVisible = usePageVisibility()
  const [mobileOpen, setMobileOpen] = useState(false)
  const [desktopOpen, setDesktopOpen] = useState(true)
  const [refreshInterval, setRefreshIntervalState] = useState(DEFAULT_REFRESH_INTERVAL)
  const [refreshKey, setRefreshKey] = useState(0)
  const refreshRequestIdRef = useRef(0)

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

  const triggerRefresh = () => {
    setRefreshKey((current) => current + 1)
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
        <TopBar
          drawerWidth={desktopOpen ? DRAWER_WIDTH : 0}
          onMenuClick={handleDrawerToggle}
          refreshInterval={refreshInterval}
          onRefreshIntervalChange={setRefreshInterval}
        />

        <Sidebar
          drawerWidth={DRAWER_WIDTH}
          mobileOpen={mobileOpen}
          desktopOpen={desktopOpen}
          onClose={handleDrawerToggle}
          isMobile={isMobile}
        />

        <Box
          component="main"
          sx={{
            flexGrow: 1,
            p: { xs: 2, sm: 3 },
            width: {
              xs: '100%',
              sm: desktopOpen ? `calc(100% - ${DRAWER_WIDTH}px)` : '100%',
            },
            mt: { xs: 7, sm: 8 },
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
      </Box>
    </RefreshContext.Provider>
  )
}
