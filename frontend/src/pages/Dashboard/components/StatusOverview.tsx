/*
 * @Author: 1orz cloudorzi@gmail.com
 * @Date: 2025-12-10 10:16:39
 * @LastEditors: 1orz cloudorzi@gmail.com
 * @LastEditTime: 2025-12-13 12:44:31
 * @FilePath: /udx710-backend/frontend/src/pages/Dashboard/components/StatusOverview.tsx
 * @Description:
 *
 * Copyright (c) 2025 by 1orz, All Rights Reserved.
 */
import { Box, Chip, Typography, Paper, useTheme, type Theme } from '@mui/material'
import { alpha } from '@/utils/theme'
import {
  SignalCellularAlt,
  WifiTethering,
  Router,
  PowerSettingsNew,
  FlightTakeoff,
  TravelExplore,
} from '@mui/icons-material'
import { formatCarrierName, getCarrierColor, getCarrierLogo } from '@/utils/carriers'
import { getSignalColor } from '../utils'
import type { DeviceInfo, NetworkInfo, CellsResponse, AirplaneModeResponse, ImsStatusResponse, RoamingResponse } from '@/api/types'

interface StatusOverviewProps {
  deviceInfo: DeviceInfo | null
  networkInfo: NetworkInfo | null
  cellsInfo: CellsResponse | null
  airplaneMode: AirplaneModeResponse | null
  imsStatus: ImsStatusResponse | null
  roaming?: RoamingResponse | null
}

export function StatusOverview({
  deviceInfo,
  networkInfo,
  cellsInfo,
  airplaneMode,
  imsStatus,
  roaming,
}: StatusOverviewProps) {
  const theme = useTheme<Theme>()

  const networkTech = (() => {
    if (cellsInfo?.serving_cell?.tech) {
      return cellsInfo.serving_cell.tech.toUpperCase()
    }
    if (networkInfo?.technology_preference) {
      if (networkInfo.technology_preference.includes('NR')) return '5G'
      if (networkInfo.technology_preference.includes('LTE')) return 'LTE'
    }
    return 'N/A'
  })()

  const signalStrength = networkInfo?.signal_strength
  const signalColor = signalStrength === undefined ? 'text.disabled' : `${getSignalColor(signalStrength)}.main`
  const registrationLabel = !networkInfo
    ? '加载中'
    : networkInfo.registration_status === 'registered'
      ? '已注册'
      : networkInfo.registration_status === 'roaming'
        ? '漫游'
        : networkInfo.registration_status || '未注册'
  const registrationColor = !networkInfo
    ? 'default'
    : networkInfo.registration_status === 'registered'
      ? 'success'
      : networkInfo.registration_status === 'roaming'
        ? 'warning'
        : 'default'
  const modemLabel = !deviceInfo ? '加载中' : deviceInfo.online ? '已上线' : '已离线'
  const modemColor = !deviceInfo ? 'default' : deviceInfo.online ? 'success' : 'error'
  const carrierLabel = networkInfo ? formatCarrierName(networkInfo.mcc, networkInfo.mnc) : '载入中'
  const carrierLogo = networkInfo ? getCarrierLogo(networkInfo.mcc, networkInfo.mnc) : null

  return (
    <Paper
      elevation={0}
      sx={{
        p: 2,
        mb: 2,
        borderRadius: 2,
        background: (() => {
          const primaryMain = (theme.palette.primary as { main: string }).main
          const secondaryMain = (theme.palette.secondary as { main: string }).main
          return `linear-gradient(135deg, ${alpha(primaryMain, 0.08)} 0%, ${alpha(secondaryMain, 0.03)} 100%)`
        })(),
        border: (() => {
          const primaryMain = (theme.palette.primary as { main: string }).main
          return `1px solid ${alpha(primaryMain, 0.1)}`
        })(),
      }}
    >
      <Box display="flex" flexWrap="wrap" alignItems="center" gap={2}>
        <Box display="flex" alignItems="center" gap={1.5}>
          {carrierLogo ? (
            <Box component="img" src={carrierLogo} alt={carrierLabel} sx={{ height: 32, width: 'auto', objectFit: 'contain' }} />
          ) : (
            <Chip
              label={carrierLabel}
              color={networkInfo ? getCarrierColor(networkInfo.mcc, networkInfo.mnc) : 'default'}
              size="small"
            />
          )}
          <Box display="flex" alignItems="center" gap={0.5}>
            <SignalCellularAlt sx={{ fontSize: 24, color: signalColor }} />
            <Typography variant="h6" fontWeight="bold" color={signalColor}>
              {signalStrength === undefined ? '--' : `${signalStrength}%`}
            </Typography>
          </Box>
        </Box>

        <Chip
          icon={<WifiTethering />}
          label={networkTech}
          color={networkTech === '5G' || networkTech === 'NR' ? 'success' : networkTech === 'N/A' ? 'default' : 'primary'}
          size="small"
          sx={{ fontWeight: 'bold' }}
        />

        <Chip icon={<Router />} label={registrationLabel} color={registrationColor} variant="outlined" size="small" />

        {roaming?.is_roaming && (
          <Chip
            icon={<TravelExplore />}
            label={roaming.roaming_allowed ? '数据漫游已开启' : '数据漫游已关闭'}
            color={roaming.roaming_allowed ? 'info' : 'error'}
            size="small"
          />
        )}

        <Chip icon={<PowerSettingsNew />} label={modemLabel} color={modemColor} size="small" />

        {imsStatus?.registered && <Chip label="VoLTE" color="info" size="small" variant="outlined" />}

        {airplaneMode?.enabled && <Chip icon={<FlightTakeoff />} label="飞行模式" color="warning" size="small" />}
      </Box>
    </Paper>
  )
}
