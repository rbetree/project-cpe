/*
 * @Author: 1orz cloudorzi@gmail.com
 * @Date: 2025-12-10 10:15:57
 * @LastEditors: 1orz cloudorzi@gmail.com
 * @LastEditTime: 2025-12-13 12:44:40
 * @FilePath: /udx710-backend/frontend/src/pages/Dashboard/hooks/useDashboardData.ts
 * @Description:
 *
 * Copyright (c) 2025 by 1orz, All Rights Reserved.
 */
import { useState, useCallback, useRef } from 'react'
import { api } from '@/api'
import {
  getAdaptiveRefreshInterval,
  useAdaptivePolling,
  usePageVisibility,
} from '@/hooks/useAdaptivePolling'
import type {
  DeviceInfo,
  NetworkInfo,
  CellsResponse,
  QosInfo,
  SimInfo,
  SystemStatsResponse,
  AirplaneModeResponse,
  ImsStatusResponse,
  RoamingResponse,
} from '@/api/types'

export const SPEED_HISTORY_MAX_POINTS = 30

export interface InterfaceSpeedHistory {
  rx: number[]
  tx: number[]
  totalRx: number
  totalTx: number
}

export interface ConnectivityResult {
  ipv4: { success: boolean; latency_ms?: number }
  ipv6: { success: boolean; latency_ms?: number }
}

export interface DashboardData {
  deviceInfo: DeviceInfo | null
  simInfo: SimInfo | null
  systemStats: SystemStatsResponse | null
  networkInfo: NetworkInfo | null
  dataStatus: boolean | null
  cellsInfo: CellsResponse | null
  qosInfo: QosInfo | null
  airplaneMode: AirplaneModeResponse | null
  imsStatus: ImsStatusResponse | null
  connectivity: ConnectivityResult | null
  speedHistory: Record<string, InterfaceSpeedHistory>
  roaming: RoamingResponse | null
}

export interface DashboardActions {
  toggleData: () => Promise<void>
  toggleAirplaneMode: () => Promise<void>
  toggleRoaming: () => Promise<void>
  loadData: () => Promise<void>
}

const CONNECTIVITY_MIN_REFRESH_INTERVAL = 15_000
const CONNECTIVITY_HIDDEN_MIN_REFRESH_INTERVAL = 60_000

export function useDashboardData(refreshInterval: number, refreshKey: number) {
  const [initialLoading, setInitialLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const [deviceInfo, setDeviceInfo] = useState<DeviceInfo | null>(null)
  const [simInfo, setSimInfo] = useState<SimInfo | null>(null)
  const [systemStats, setSystemStats] = useState<SystemStatsResponse | null>(null)
  const [networkInfo, setNetworkInfo] = useState<NetworkInfo | null>(null)
  const [dataStatus, setDataStatus] = useState<boolean | null>(null)
  const [cellsInfo, setCellsInfo] = useState<CellsResponse | null>(null)
  const [qosInfo, setQosInfo] = useState<QosInfo | null>(null)
  const [airplaneMode, setAirplaneMode] = useState<AirplaneModeResponse | null>(null)
  const [imsStatus, setImsStatus] = useState<ImsStatusResponse | null>(null)
  const [connectivity, setConnectivity] = useState<ConnectivityResult | null>(null)
  const [roaming, setRoaming] = useState<RoamingResponse | null>(null)

  const [speedHistory, setSpeedHistory] = useState<Record<string, InterfaceSpeedHistory>>({})
  const speedHistoryRef = useRef<Record<string, InterfaceSpeedHistory>>({})
  const requestIdRef = useRef(0)

  const updateSpeedHistory = useCallback((stats: SystemStatsResponse | null) => {
    if (!stats?.network_speed?.interfaces) return

    const newHistory = { ...speedHistoryRef.current }

    for (const iface of stats.network_speed.interfaces) {
      const existing = newHistory[iface.interface] || { rx: [], tx: [], totalRx: 0, totalTx: 0 }

      const rxHistory = [...existing.rx, iface.rx_bytes_per_sec]
      const txHistory = [...existing.tx, iface.tx_bytes_per_sec]

      if (rxHistory.length > SPEED_HISTORY_MAX_POINTS) {
        rxHistory.shift()
        txHistory.shift()
      }

      newHistory[iface.interface] = {
        rx: rxHistory,
        tx: txHistory,
        totalRx: iface.total_rx_bytes,
        totalTx: iface.total_tx_bytes,
      }
    }

    speedHistoryRef.current = newHistory
    setSpeedHistory(newHistory)
  }, [])

  const formatRequestError = useCallback((label: string, errorValue: unknown) => {
    const message = errorValue instanceof Error ? errorValue.message : String(errorValue)
    return `${label}: ${message}`
  }, [])

  const isPageVisible = usePageVisibility()
  const connectivityRefreshInterval = getAdaptiveRefreshInterval(
    Math.max(refreshInterval, CONNECTIVITY_MIN_REFRESH_INTERVAL),
    isPageVisible,
    {
      hiddenMinInterval: CONNECTIVITY_HIDDEN_MIN_REFRESH_INTERVAL,
      hiddenMultiplier: 4,
    }
  )

  const loadData = useCallback(async () => {
    const requestId = requestIdRef.current + 1
    requestIdRef.current = requestId
    setError(null)

    const extendedRequests = Promise.allSettled([api.getImsStatus(), api.getRoamingStatus()])

    const baseResults = await Promise.allSettled([
      api.getDeviceInfo(),
      api.getSimInfo(),
      api.getSystemStats(),
      api.getNetworkInfo(),
      api.getDataStatus(),
      api.getCellsInfo(),
      api.getQosInfo(),
      api.getAirplaneMode(),
    ])

    if (requestId !== requestIdRef.current) {
      return
    }

    const baseErrors: string[] = []

    const deviceResult = baseResults[0]
    if (deviceResult.status === 'fulfilled') {
      if (deviceResult.value.data) setDeviceInfo(deviceResult.value.data)
    } else {
      baseErrors.push(formatRequestError('设备信息', deviceResult.reason))
    }

    const simResult = baseResults[1]
    if (simResult.status === 'fulfilled') {
      if (simResult.value.data) setSimInfo(simResult.value.data)
    } else {
      baseErrors.push(formatRequestError('SIM 信息', simResult.reason))
    }

    const statsResult = baseResults[2]
    if (statsResult.status === 'fulfilled') {
      if (statsResult.value.data) {
        setSystemStats(statsResult.value.data)
        updateSpeedHistory(statsResult.value.data)
      }
    } else {
      baseErrors.push(formatRequestError('系统状态', statsResult.reason))
    }

    const networkResult = baseResults[3]
    if (networkResult.status === 'fulfilled') {
      if (networkResult.value.data) setNetworkInfo(networkResult.value.data)
    } else {
      baseErrors.push(formatRequestError('网络信息', networkResult.reason))
    }

    const dataStatusResult = baseResults[4]
    if (dataStatusResult.status === 'fulfilled') {
      if (dataStatusResult.value.data) setDataStatus(dataStatusResult.value.data.active)
    } else {
      baseErrors.push(formatRequestError('数据连接状态', dataStatusResult.reason))
    }

    const cellsResult = baseResults[5]
    if (cellsResult.status === 'fulfilled') {
      if (cellsResult.value.data) setCellsInfo(cellsResult.value.data)
    } else {
      baseErrors.push(formatRequestError('小区信息', cellsResult.reason))
    }

    const qosResult = baseResults[6]
    if (qosResult.status === 'fulfilled') {
      if (qosResult.value.data) setQosInfo(qosResult.value.data)
    } else {
      baseErrors.push(formatRequestError('QoS 信息', qosResult.reason))
    }

    const airplaneModeResult = baseResults[7]
    if (airplaneModeResult.status === 'fulfilled') {
      if (airplaneModeResult.value.data) setAirplaneMode(airplaneModeResult.value.data)
    } else {
      baseErrors.push(formatRequestError('飞行模式状态', airplaneModeResult.reason))
    }

    setInitialLoading(false)
    if (baseErrors.length > 0) {
      setError(baseErrors[0])
    }

    const extendedResults = await extendedRequests
    if (requestId !== requestIdRef.current) {
      return
    }

    const extendedErrors: string[] = []

    const imsResult = extendedResults[0]
    if (imsResult.status === 'fulfilled') {
      if (imsResult.value.data) setImsStatus(imsResult.value.data)
    } else {
      extendedErrors.push(formatRequestError('IMS 状态', imsResult.reason))
    }

    const roamingResult = extendedResults[1]
    if (roamingResult.status === 'fulfilled') {
      if (roamingResult.value.data) setRoaming(roamingResult.value.data)
    } else {
      extendedErrors.push(formatRequestError('漫游状态', roamingResult.reason))
    }

    if (extendedErrors.length > 0 && baseErrors.length === 0) {
      setError(extendedErrors[0])
    }
  }, [formatRequestError, updateSpeedHistory])

  const loadConnectivity = useCallback(async () => {
    try {
      const response = await api.getConnectivity()
      if (response.data) {
        setConnectivity(response.data)
      }
    } catch (errorValue) {
      setError((currentError) => currentError ?? formatRequestError('连通性检测', errorValue))
    }
  }, [formatRequestError])

  const toggleData = useCallback(async () => {
    if (dataStatus === null) return

    try {
      const newStatus = !dataStatus
      await api.setDataStatus(newStatus)
      setDataStatus(newStatus)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }, [dataStatus])

  const toggleAirplaneMode = useCallback(async () => {
    if (!airplaneMode) return

    try {
      const newEnabled = !airplaneMode.enabled
      const response = await api.setAirplaneMode(newEnabled)
      if (response.data) {
        setAirplaneMode(response.data)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }, [airplaneMode])

  const toggleRoaming = useCallback(async () => {
    if (!roaming) return

    try {
      const newAllowed = !roaming.roaming_allowed
      const response = await api.setRoamingAllowed(newAllowed)
      if (response.data) {
        setRoaming(response.data)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }, [roaming])

  useAdaptivePolling({
    refreshInterval,
    refreshKey,
    onTick: loadData,
  })

  useAdaptivePolling({
    refreshInterval: connectivityRefreshInterval,
    refreshKey,
    onTick: loadConnectivity,
  })

  return {
    initialLoading,
    error,
    setError,
    data: {
      deviceInfo,
      simInfo,
      systemStats,
      networkInfo,
      dataStatus,
      cellsInfo,
      qosInfo,
      airplaneMode,
      imsStatus,
      connectivity,
      speedHistory,
      roaming,
    } as DashboardData,
    actions: {
      toggleData,
      toggleAirplaneMode,
      toggleRoaming,
      loadData,
    } as DashboardActions,
  }
}
