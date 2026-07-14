import { useEffect, useMemo, useState } from 'react'

const DEFAULT_HIDDEN_MIN_INTERVAL = 30_000
const DEFAULT_HIDDEN_MULTIPLIER = 6

interface AdaptiveIntervalOptions {
  hiddenMinInterval?: number
  hiddenMultiplier?: number
}

interface AdaptivePollingOptions extends AdaptiveIntervalOptions {
  refreshInterval: number
  refreshKey?: number
  onTick: () => void | Promise<void>
  immediate?: boolean
}

export function usePageVisibility() {
  const [isPageVisible, setIsPageVisible] = useState(() =>
    typeof document === 'undefined' ? true : !document.hidden
  )

  useEffect(() => {
    if (typeof document === 'undefined') {
      return undefined
    }

    const handleVisibilityChange = () => {
      setIsPageVisible(!document.hidden)
    }

    document.addEventListener('visibilitychange', handleVisibilityChange)
    return () => {
      document.removeEventListener('visibilitychange', handleVisibilityChange)
    }
  }, [])

  return isPageVisible
}

export function getAdaptiveRefreshInterval(
  refreshInterval: number,
  isPageVisible: boolean,
  {
    hiddenMinInterval = DEFAULT_HIDDEN_MIN_INTERVAL,
    hiddenMultiplier = DEFAULT_HIDDEN_MULTIPLIER,
  }: AdaptiveIntervalOptions = {}
) {
  if (refreshInterval <= 0) {
    return 0
  }

  if (isPageVisible) {
    return refreshInterval
  }

  return Math.max(refreshInterval * hiddenMultiplier, hiddenMinInterval)
}

export function useAdaptivePolling({
  refreshInterval,
  refreshKey = 0,
  onTick,
  immediate = true,
  hiddenMinInterval,
  hiddenMultiplier,
}: AdaptivePollingOptions) {
  const isPageVisible = usePageVisibility()
  const effectiveRefreshInterval = useMemo(
    () =>
      getAdaptiveRefreshInterval(refreshInterval, isPageVisible, {
        hiddenMinInterval,
        hiddenMultiplier,
      }),
    [hiddenMinInterval, hiddenMultiplier, isPageVisible, refreshInterval]
  )

  useEffect(() => {
    if (immediate) {
      void onTick()
    }

    if (effectiveRefreshInterval <= 0) {
      return undefined
    }

    const timer = window.setInterval(() => {
      void onTick()
    }, effectiveRefreshInterval)

    return () => {
      window.clearInterval(timer)
    }
  }, [effectiveRefreshInterval, immediate, onTick, refreshKey])

  return {
    effectiveRefreshInterval,
    isPageVisible,
  }
}
