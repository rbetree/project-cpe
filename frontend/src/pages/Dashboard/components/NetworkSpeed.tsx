/*
 * @Author: 1orz cloudorzi@gmail.com
 * @Date: 2025-12-10 10:17:51
 * @LastEditors: 1orz cloudorzi@gmail.com
 * @LastEditTime: 2025-12-13 12:44:25
 * @FilePath: /udx710-backend/frontend/src/pages/Dashboard/components/NetworkSpeed.tsx
 * @Description:
 *
 * Copyright (c) 2025 by 1orz, All Rights Reserved.
 */
import { useId } from 'react'
import { Box, Card, CardContent, Typography, Stack, Chip, Paper, useTheme, type Theme } from '@mui/material'
import { alpha } from '@/utils/theme'
import { Speed, ArrowDownward, ArrowUpward } from '@mui/icons-material'
import { formatBytes, formatSpeed } from '../utils'
import { SPEED_HISTORY_MAX_POINTS, type InterfaceSpeedHistory } from '../hooks/useDashboardData'
import type { SystemStatsResponse } from '@/api/types'

interface NetworkSpeedProps {
  systemStats: SystemStatsResponse | null
  speedHistory: Record<string, InterfaceSpeedHistory>
}

function SparkLine({
  data,
  height = 40,
  color,
  max,
}: {
  data: number[]
  height?: number
  color: string
  max: number
}) {
  const id = useId()
  if (data.length < 2) return null

  const minVal = 0
  const maxVal = max || Math.max(...data, 1)
  const range = maxVal - minVal || 1

  const viewWidth = Math.max(data.length - 1, 1)

  const linePoints = data
    .map((value, index) => {
      const x = (index / (data.length - 1)) * viewWidth
      const y = height - ((value - minVal) / range) * height
      return `${x},${y}`
    })
    .join(' ')

  const areaPoints = `0,${height} ${linePoints} ${viewWidth},${height}`

  return (
    <svg width="100%" height={height} viewBox={`0 0 ${viewWidth} ${height}`} preserveAspectRatio="none">
      <defs>
        <linearGradient id={`${id}-gradient`} x1="0" x2="0" y1="0" y2="1">
          <stop offset="0%" stopColor={color} stopOpacity={0.3} />
          <stop offset="100%" stopColor={color} stopOpacity={0.02} />
        </linearGradient>
      </defs>
      <polyline points={areaPoints} fill={`url(#${id}-gradient)`} stroke="none" />
      <polyline
        points={linePoints}
        fill="none"
        stroke={color}
        strokeWidth={1.5}
        strokeLinejoin="round"
        strokeLinecap="round"
      />
    </svg>
  )
}

export function NetworkSpeed({ systemStats, speedHistory }: NetworkSpeedProps) {
  const theme = useTheme<Theme>()

  return (
    <Card sx={{ height: '100%', overflow: 'hidden' }}>
      <CardContent>
        <Box display="flex" alignItems="center" gap={1} mb={2}>
          <Speed color="primary" />
          <Typography variant="subtitle2" color="text.secondary">
            实时网速
          </Typography>
          <Typography variant="caption" color="text.disabled" sx={{ ml: 'auto' }}>
            {SPEED_HISTORY_MAX_POINTS}s 趋势
          </Typography>
        </Box>
        {systemStats?.network_speed?.interfaces && systemStats.network_speed.interfaces.length > 0 ? (
          <Stack spacing={2}>
            {systemStats.network_speed.interfaces.map((iface) => {
              const history = speedHistory[iface.interface]
              const rxData = history?.rx || []
              const txData = history?.tx || []
              const maxSpeed = Math.max(Math.max(...rxData, 1), Math.max(...txData, 1))

              return (
                <Paper
                  key={iface.interface}
                  variant="outlined"
                  sx={{
                    p: 2,
                    overflow: 'hidden',
                    background: (() => {
                      const paperColor = (theme.palette.background as { paper: string }).paper
                      return alpha(paperColor, 0.6)
                    })(),
                  }}
                >
                  <Box display="flex" alignItems="center" justifyContent="space-between" mb={1.5}>
                    <Chip label={iface.interface} size="small" variant="outlined" sx={{ fontFamily: 'monospace', fontWeight: 500 }} />
                    <Typography variant="caption" color="text.secondary">
                      总流量 ↓ {formatBytes(iface.total_rx_bytes)} / ↑ {formatBytes(iface.total_tx_bytes)}
                    </Typography>
                  </Box>

                  <Box mb={1.5}>
                    <Box display="flex" alignItems="center" justifyContent="space-between" mb={0.5}>
                      <Box display="flex" alignItems="center" gap={0.5}>
                        <ArrowDownward fontSize="small" sx={{ color: (theme.palette.success as { main: string }).main }} />
                        <Typography variant="caption" color="text.secondary">
                          下载
                        </Typography>
                      </Box>
                      <Typography
                        variant="body1"
                        fontWeight="bold"
                        sx={{
                          color: (theme.palette.success as { main: string }).main,
                          fontFamily: 'monospace',
                          minWidth: 90,
                          textAlign: 'right',
                        }}
                      >
                        {formatSpeed(iface.rx_bytes_per_sec)}
                      </Typography>
                    </Box>
                    {rxData.length > 1 && (
                      <Box sx={{ height: 40, width: '100%' }}>
                        <SparkLine
                          data={rxData}
                          height={40}
                          color={(theme.palette.success as { main: string }).main}
                          max={maxSpeed * 1.1}
                        />
                      </Box>
                    )}
                  </Box>

                  <Box>
                    <Box display="flex" alignItems="center" justifyContent="space-between" mb={0.5}>
                      <Box display="flex" alignItems="center" gap={0.5}>
                        <ArrowUpward fontSize="small" sx={{ color: (theme.palette.primary as { main: string }).main }} />
                        <Typography variant="caption" color="text.secondary">
                          上传
                        </Typography>
                      </Box>
                      <Typography
                        variant="body1"
                        fontWeight="bold"
                        sx={{
                          color: (theme.palette.primary as { main: string }).main,
                          fontFamily: 'monospace',
                          minWidth: 90,
                          textAlign: 'right',
                        }}
                      >
                        {formatSpeed(iface.tx_bytes_per_sec)}
                      </Typography>
                    </Box>
                    {txData.length > 1 && (
                      <Box sx={{ height: 40, width: '100%' }}>
                        <SparkLine
                          data={txData}
                          height={40}
                          color={(theme.palette.primary as { main: string }).main}
                          max={maxSpeed * 1.1}
                        />
                      </Box>
                    )}
                  </Box>
                </Paper>
              )
            })}
          </Stack>
        ) : (
          <Typography variant="body2" color="text.secondary">
            暂无数据
          </Typography>
        )}
      </CardContent>
    </Card>
  )
}
