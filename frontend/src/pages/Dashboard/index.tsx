/*
 * @Author: 1orz cloudorzi@gmail.com
 * @Date: 2025-12-10 10:22:12
 * @LastEditors: 1orz cloudorzi@gmail.com
 * @LastEditTime: 2025-12-13 12:44:42
 * @FilePath: /udx710-backend/frontend/src/pages/Dashboard/index.tsx
 * @Description:
 *
 * Copyright (c) 2025 by 1orz, All Rights Reserved.
 */
import { Box, Card, CardContent, Typography, LinearProgress } from '@mui/material'
import Grid from '@mui/material/Grid'
import { useRefreshInterval } from '@/contexts/RefreshContext'
import ErrorSnackbar from '@/components/ErrorSnackbar'
import PageContainer from '@/components/Page/PageContainer'
import { useDashboardData } from './hooks/useDashboardData'
import {
  StatusOverview,
  QuickControls,
  SystemResources,
  NetworkSpeed,
  ConnectionStatus,
  SimCardInfo,
  TemperatureMonitor,
  CellInfo,
  DeviceInfoCard,
} from './components'

/** 阶段 2 数据加载中的占位卡片 */
function SlowDataPlaceholder() {
  return (
    <Card sx={{ height: '100%', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
      <CardContent sx={{ width: '100%', textAlign: 'center' }}>
        <LinearProgress sx={{ height: 2, borderRadius: 999, mb: 1.5 }} />
        <Typography variant="caption" color="text.disabled">
          加载中...
        </Typography>
      </CardContent>
    </Card>
  )
}

export default function Dashboard() {
  const { refreshInterval, refreshKey } = useRefreshInterval()
  const { fastDataReady, allDataReady, error, setError, data, actions } = useDashboardData(refreshInterval, refreshKey)

  return (
    <PageContainer pageId="dashboard">
      <ErrorSnackbar error={error} onClose={() => setError(null)} />

      {/* 阶段 1 全屏 loading */}
      {!fastDataReady && (
        <Box sx={{ mb: 2 }}>
          <LinearProgress sx={{ height: 3, borderRadius: 999 }} />
        </Box>
      )}

      {/* 顶部状态概览（依赖少量慢数据，字段优雅降级显示 '-'） */}
      <StatusOverview
        deviceInfo={data.deviceInfo}
        networkInfo={data.networkInfo}
        cellsInfo={data.cellsInfo}
        airplaneMode={data.airplaneMode}
        imsStatus={data.imsStatus}
        roaming={data.roaming}
      />

      {/* 主体内容区，PC 端采用多列布局 */}
      <Grid container spacing={2}>
        {/* 第一行：快捷控制、连接状态、SIM 信息、系统资源 */}
        <Grid size={{ xs: 12, sm: 6, md: 3 }}>
          <QuickControls
            dataStatus={data.dataStatus}
            airplaneMode={data.airplaneMode}
            roaming={data.roaming}
            onToggleData={() => void actions.toggleData()}
            onToggleAirplaneMode={() => void actions.toggleAirplaneMode()}
            onToggleRoaming={() => void actions.toggleRoaming()}
          />
        </Grid>

        <Grid size={{ xs: 12, sm: 6, md: 3 }}>
          {allDataReady ? (
            <ConnectionStatus qosInfo={data.qosInfo} connectivity={data.connectivity} />
          ) : (
            <SlowDataPlaceholder />
          )}
        </Grid>

        <Grid size={{ xs: 12, sm: 6, md: 3 }}>
          <SimCardInfo simInfo={data.simInfo} />
        </Grid>

        <Grid size={{ xs: 12, sm: 6, md: 3 }}>
          {allDataReady ? (
            <SystemResources systemStats={data.systemStats} />
          ) : (
            <SlowDataPlaceholder />
          )}
        </Grid>

        {/* 第二行：实时网速、温度监控 */}
        <Grid size={{ xs: 12, md: 8 }}>
          {allDataReady ? (
            <NetworkSpeed systemStats={data.systemStats} speedHistory={data.speedHistory} />
          ) : (
            <SlowDataPlaceholder />
          )}
        </Grid>

        <Grid size={{ xs: 12, md: 4 }}>
          {allDataReady ? (
            <TemperatureMonitor systemStats={data.systemStats} />
          ) : (
            <SlowDataPlaceholder />
          )}
        </Grid>

        {/* 第三行：小区信息（全宽） */}
        <Grid size={12}>
          {allDataReady ? (
            <CellInfo cellsInfo={data.cellsInfo} />
          ) : (
            <SlowDataPlaceholder />
          )}
        </Grid>

        {/* 第四行：设备信息（全宽） — 混合快慢数据，慢字段优雅降级 */}
        <Grid size={12}>
          <DeviceInfoCard deviceInfo={data.deviceInfo} systemStats={data.systemStats} />
        </Grid>
      </Grid>
    </PageContainer>
  )
}
