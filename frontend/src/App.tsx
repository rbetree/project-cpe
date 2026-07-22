/*
 * @Author: 1orz cloudorzi@gmail.com
 * @Date: 2025-12-10 09:19:05
 * @LastEditors: 1orz cloudorzi@gmail.com
 * @LastEditTime: 2025-12-13 12:45:12
 * @FilePath: /udx710-backend/frontend/src/App.tsx
 * @Description: 
 * 
 * Copyright (c) 2025 by 1orz, All Rights Reserved. 
 */
import { lazy, Suspense, type ComponentType, type LazyExoticComponent } from 'react'
import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom'
import { Box, CircularProgress } from '@mui/material'
import { ThemeProvider } from './contexts/ThemeContext'
import MainLayout from './components/Layout/MainLayout'
import { appPages, type AppPageId } from './navigation'

// 路由级别代码分割 - 按需加载页面组件
const Dashboard = lazy(() => import('./pages/Dashboard'))
const DeviceInfo = lazy(() => import('./pages/DeviceInfo'))
const Network = lazy(() => import('./pages/Network'))
const Phone = lazy(() => import('./pages/Phone'))
const SMS = lazy(() => import('./pages/SMS'))
const Configuration = lazy(() => import('./pages/Configuration'))
const InitScript = lazy(() => import('./pages/InitScript'))
const ATConsole = lazy(() => import('./pages/ATConsole'))
const Terminal = lazy(() => import('./pages/Terminal'))
const OtaUpdate = lazy(() => import('./pages/OtaUpdate'))
const Logs = lazy(() => import('./pages/Logs'))

// 页面加载中的 fallback
function PageLoading() {
  return (
    <Box display="flex" justifyContent="center" alignItems="center" minHeight="50vh">
      <CircularProgress size={32} />
    </Box>
  )
}

type LazyPageComponent = LazyExoticComponent<ComponentType>

interface AppRouteConfig {
  pageId: AppPageId
  component: LazyPageComponent
}

const appRoutes: AppRouteConfig[] = [
  { pageId: 'dashboard', component: Dashboard },
  { pageId: 'device', component: DeviceInfo },
  { pageId: 'network', component: Network },
  { pageId: 'phone', component: Phone },
  { pageId: 'sms', component: SMS },
  { pageId: 'config', component: Configuration },
  { pageId: 'initScript', component: InitScript },
  { pageId: 'ota', component: OtaUpdate },
  { pageId: 'logs', component: Logs },
  { pageId: 'atConsole', component: ATConsole },
  { pageId: 'terminal', component: Terminal },
]

function getRoutePath(pageId: AppPageId) {
  const page = appPages.find((item) => item.id === pageId)
  if (!page || page.path === '/') {
    return undefined
  }

  return page.path.replace(/^\//, '')
}

function renderLazyPage(PageComponent: LazyPageComponent) {
  return (
    <Suspense fallback={<PageLoading />}>
      <PageComponent />
    </Suspense>
  )
}

function App() {
  return (
    <ThemeProvider>
      <BrowserRouter>
        <Routes>
          <Route path="/" element={<MainLayout />}>
            {appRoutes.map((route) => (
              <Route
                key={route.pageId}
                index={getRoutePath(route.pageId) === undefined}
                path={getRoutePath(route.pageId)}
                element={renderLazyPage(route.component)}
              />
            ))}
            {/* 旧路由重定向到网络状态页面 */}
            <Route path="network-interfaces" element={<Navigate to="/network" replace />} />
            <Route path="band-lock" element={<Navigate to="/network" replace />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </ThemeProvider>
  )
}

export default App
