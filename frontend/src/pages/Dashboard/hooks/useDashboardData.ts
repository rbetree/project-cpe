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

/** 快速 API（首屏可见） */
const FAST_REQUEST_FACTORIES = [
  () => api.getDeviceInfo(),
  () => api.getSimInfo(),
  () => api.getNetworkInfo(),
  () => api.getDataStatus(),
  () => api.getAirplaneMode(),
  () => api.getRoamingStatus(),
] as const

const FAST_LABELS = ['设备信息', 'SIM 信息', '网络信息', '数据连接状态', '飞行模式状态', '漫游状态'] as const

/** 慢速 API（AT 命令，延迟加载） */
const SLOW_REQUEST_FACTORIES = [
  () => api.getCellsInfo(),
  () => api.getQosInfo(),
  () => api.getSystemStats(),
  () => api.getImsStatus(),
] as const

const SLOW_LABELS = ['小区信息', 'QoS 信息', '系统状态', 'IMS 状态'] as const

const CONNECTIVITY_MIN_REFRESH_INTERVAL = 15_000
const CONNECTIVITY_HIDDEN_MIN_REFRESH_INTERVAL = 60_000

export function useDashboardData(refreshInterval: number, refreshKey: number) {
  const [fastDataReady, setFastDataReady] = useState(false)
  const [allDataReady, setAllDataReady] = useState(false)
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

    // 同时发起所有请求，但分阶段处理结果
    const fastRequests = Promise.allSettled(FAST_REQUEST_FACTORIES.map((fn) => fn()))
    const slowRequests = Promise.allSettled(SLOW_REQUEST_FACTORIES.map((fn) => fn()))

    // ── 阶段 1：处理快速数据 ──
    const fastResults = await fastRequests
    if (requestId !== requestIdRef.current) return

    const fastErrors: string[] = []

    for (let i = 0; i < fastResults.length; i++) {
      const result = fastResults[i]
      if (result.status === 'fulfilled') {
        const data = result.value.data
        if (!data) continue
        switch (i) {
          case 0: setDeviceInfo(data as DeviceInfo); break
          case 1: setSimInfo(data as SimInfo); break
          case 2: setNetworkInfo(data as NetworkInfo); break
          case 3: setDataStatus((data as { active: boolean }).active); break
          case 4: setAirplaneMode(data as AirplaneModeResponse); break
          case 5: setRoaming(data as RoamingResponse); break
        }
      } else {
        fastErrors.push(formatRequestError(FAST_LABELS[i], result.reason))
      }
    }

    setFastDataReady(true)
    if (fastErrors.length > 0) {
      setError(fastErrors[0])
    }

    // ── 阶段 2：处理慢速数据 ──
    const slowResults = await slowRequests
    if (requestId !== requestIdRef.current) return

    const slowErrors: string[] = []

    for (let i = 0; i < slowResults.length; i++) {
      const result = slowResults[i]
      if (result.status === 'fulfilled') {
        const data = result.value.data
        if (!data) continue
        switch (i) {
          case 0: setCellsInfo(data as CellsResponse); break
          case 1: setQosInfo(data as QosInfo); break
          case 2: {
            const stats = data as SystemStatsResponse
            setSystemStats(stats)
            updateSpeedHistory(stats)
            break
          }
          case 3: setImsStatus(data as ImsStatusResponse); break
        }
      } else {
        slowErrors.push(formatRequestError(SLOW_LABELS[i], result.reason))
      }
    }

    setAllDataReady(true)
    if (slowErrors.length > 0 && fastErrors.length === 0) {
      setError(slowErrors[0])
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
    fastDataReady,
    allDataReady,
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
