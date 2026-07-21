/*
 * @Description: 日志页面 —— 方向A 远程上报配置 + 方向B 实时查看/导出
 *   - 上半：配置（远程上报 / 现场查看 / 缓冲容量），含测试与丢弃统计
 *   - 下半：实时日志查看器（SSE、级别过滤、暂停、清屏、导出下载）
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  Box,
  Typography,
  Card,
  CardContent,
  CardHeader,
  Switch,
  FormControlLabel,
  TextField,
  Button,
  MenuItem,
  Alert,
  CircularProgress,
  Chip,
  IconButton,
  Stack,
  Tooltip,
  Divider,
  ToggleButton,
  ToggleButtonGroup,
} from '@mui/material'
import Grid from '@mui/material/Grid'
import {
  PlayArrow,
  Pause,
  Delete,
  Download,
  Send,
  Save,
  Refresh,
} from '@mui/icons-material'
import { api } from '../api'
import ErrorSnackbar from '../components/ErrorSnackbar'
import type { LogExportConfig, LogEntry, LogLevel } from '../api/types'
import { LOG_LEVEL_OPTIONS } from '../api/types'

// 默认配置（与后端 Default 对齐）
const DEFAULT_CONFIG: LogExportConfig = {
  remote_enabled: false,
  remote_url: '',
  remote_token: '',
  remote_level: 'info',
  batch_size: 100,
  flush_interval_ms: 5000,
  viewer_enabled: true,
  viewer_level: 'info',
  buffer_capacity: 2000,
}

// 级别 → 严重度数字（越小越严重）
const LEVEL_SEVERITY: Record<string, number> = {
  ERROR: 1,
  WARN: 2,
  INFO: 3,
  DEBUG: 4,
  TRACE: 5,
}

const LEVEL_COLOR: Record<string, 'error' | 'warning' | 'info' | 'default' | 'success'> = {
  ERROR: 'error',
  WARN: 'warning',
  INFO: 'info',
  DEBUG: 'default',
  TRACE: 'success',
}

// 浏览器侧保留的最大条数（防止内存膨胀；设备侧另由缓冲容量约束）
const MAX_VIEWER_ENTRIES = 1000

export default function Logs() {
  const [config, setConfig] = useState<LogExportConfig>(DEFAULT_CONFIG)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [testing, setTesting] = useState(false)
  const [testMessage, setTestMessage] = useState<{ success: boolean; message: string } | null>(null)
  const [success, setSuccess] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [droppedOverflow, setDroppedOverflow] = useState(0)
  const [droppedRemote, setDroppedRemote] = useState(0)

  // 实时查看器
  const [entries, setEntries] = useState<LogEntry[]>([])
  const [paused, setPaused] = useState(false)
  const [minLevel, setMinLevel] = useState<LogLevel>('debug')
  const pausedRef = useRef(paused)
  const viewerRef = useRef<HTMLDivElement>(null)
  const [autoScroll, setAutoScroll] = useState(true)

  // 在 effect 中同步 ref（避免 render 期写 ref）
  useEffect(() => {
    pausedRef.current = paused
  }, [paused])

  const loadConfig = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const res = await api.getLogsConfig()
      if (res.data) {
        setConfig(res.data)
        setDroppedOverflow(res.data.dropped_overflow ?? 0)
        setDroppedRemote(res.data.dropped_remote ?? 0)
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void loadConfig()
  }, [loadConfig])

  // SSE 实时流
  useEffect(() => {
    if (!config.viewer_enabled) {
      return
    }
    const es = api.streamLogs()
    es.addEventListener('log', (ev) => {
      if (pausedRef.current) return
      try {
        const entry = JSON.parse(ev.data) as LogEntry
        setEntries((prev) => {
          const next = prev.length >= MAX_VIEWER_ENTRIES ? prev.slice(prev.length - MAX_VIEWER_ENTRIES + 1) : prev
          return [...next, entry]
        })
      } catch {
        // 忽略解析失败
      }
    })
    es.addEventListener('lag', (ev) => {
      if (pausedRef.current) return
      setEntries((prev) => {
        const lagEntry: LogEntry = {
          ts: new Date().toISOString(),
          level: 'WARN',
          target: 'viewer',
          message: `⚠ 实时流积压，部分日志被跳过（${ev.data}）`,
          fields: '',
        }
        return [...prev, lagEntry]
      })
    })
    es.onerror = () => {
      // EventSource 会自动重连，这里不额外提示
    }
    return () => {
      es.close()
    }
  }, [config.viewer_enabled])

  // 自动滚动到底
  useEffect(() => {
    if (autoScroll && viewerRef.current) {
      viewerRef.current.scrollTop = viewerRef.current.scrollHeight
    }
  }, [entries, autoScroll])

  const handleSave = useCallback(async () => {
    setSaving(true)
    setError(null)
    setSuccess(null)
    try {
      const res = await api.setLogsConfig(config)
      if (res.status === 'ok') {
        setSuccess('日志配置已保存并生效')
        // 重新拉取以同步丢弃统计
        await loadConfig()
      } else {
        setError(res.message || '保存失败')
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setSaving(false)
    }
  }, [config, loadConfig])

  const handleTest = useCallback(async () => {
    setTesting(true)
    setTestMessage(null)
    try {
      const res = await api.testLogsRemote()
      if (res.data) {
        setTestMessage({ success: res.data.success, message: res.data.message })
      }
    } catch (e) {
      setTestMessage({ success: false, message: e instanceof Error ? e.message : String(e) })
    } finally {
      setTesting(false)
    }
  }, [])

  const handleClear = useCallback(async () => {
    try {
      await api.clearLogs()
      setEntries([])
      setSuccess('已清空缓冲日志')
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [])

  const handleExport = useCallback(async (format: 'text' | 'json') => {
    try {
      const blob = await api.exportLogs(format)
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = format === 'json' ? 'udx710-logs.json' : 'udx710.log'
      document.body.appendChild(a)
      a.click()
      document.body.removeChild(a)
      URL.revokeObjectURL(url)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [])

  // 配置字段更新助手
  const update = useCallback(<K extends keyof LogExportConfig>(key: K, value: LogExportConfig[K]) => {
    setConfig((prev) => ({ ...prev, [key]: value }))
  }, [])

  // 过滤后的查看器条目
  const visibleEntries = useMemo(() => {
    const threshold = LEVEL_SEVERITY[minLevel.toUpperCase()] ?? 5
    return entries.filter((e) => (LEVEL_SEVERITY[e.level] ?? 5) <= threshold)
  }, [entries, minLevel])

  // 缓冲容量内存估算（~400B/条，且受 1 MiB 字节硬顶）
  const bufferMemKB = useMemo(() => {
    const approx = config.buffer_capacity * 400
    return Math.min(approx, 1024 * 1024) / 1024
  }, [config.buffer_capacity])

  return (
    <Box sx={{ p: 3, display: 'flex', flexDirection: 'column', gap: 2 }}>
      <Box sx={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <Typography variant="h5">日志</Typography>
        <Button size="small" startIcon={<Refresh />} onClick={() => void loadConfig()} disabled={loading}>
          刷新
        </Button>
      </Box>

      {loading ? (
        <Box sx={{ display: 'flex', justifyContent: 'center', py: 4 }}>
          <CircularProgress />
        </Box>
      ) : (
        <>
          {/* ===== 配置卡片 ===== */}
          <Card>
            <CardHeader title="日志配置" subheader="设备零磁盘：日志仅在内存有界缓冲，可远程上报或现场查看/导出" />
            <CardContent>
              <Grid container spacing={2}>
                {/* 远程上报 */}
                <Grid size={{ xs: 12 }}>
                  <FormControlLabel
                    control={
                      <Switch
                        checked={config.remote_enabled}
                        onChange={(_, v) => update('remote_enabled', v)}
                      />
                    }
                    label="远程上报（方向A：异步批量 POST 到外部端点）"
                  />
                </Grid>
                <Grid size={{ xs: 12, md: 8 }}>
                  <TextField
                    fullWidth
                    size="small"
                    label="远程上报 URL（HTTPS 推荐）"
                    placeholder="https://your-log-collector.example.com/ingest"
                    value={config.remote_url}
                    onChange={(e) => update('remote_url', e.target.value)}
                    disabled={!config.remote_enabled}
                  />
                </Grid>
                <Grid size={{ xs: 12, md: 4 }}>
                  <TextField
                    fullWidth
                    size="small"
                    label="Bearer Token（可选）"
                    type="password"
                    value={config.remote_token}
                    onChange={(e) => update('remote_token', e.target.value)}
                    disabled={!config.remote_enabled}
                  />
                </Grid>
                <Grid size={{ xs: 12, sm: 4 }}>
                  <TextField
                    select
                    fullWidth
                    size="small"
                    label="上报级别"
                    value={config.remote_level}
                    onChange={(e) => update('remote_level', e.target.value as LogLevel)}
                    disabled={!config.remote_enabled}
                  >
                    {LOG_LEVEL_OPTIONS.map((l) => (
                      <MenuItem key={l} value={l}>
                        {l}
                      </MenuItem>
                    ))}
                  </TextField>
                </Grid>
                <Grid size={{ xs: 6, sm: 4 }}>
                  <TextField
                    fullWidth
                    size="small"
                    type="number"
                    label="单批条数 (1–500)"
                    inputProps={{ min: 1, max: 500 }}
                    value={config.batch_size}
                    onChange={(e) => update('batch_size', Number(e.target.value))}
                    disabled={!config.remote_enabled}
                  />
                </Grid>
                <Grid size={{ xs: 6, sm: 4 }}>
                  <TextField
                    fullWidth
                    size="small"
                    type="number"
                    label="flush 间隔 ms (500–60000)"
                    inputProps={{ min: 500, max: 60000, step: 500 }}
                    value={config.flush_interval_ms}
                    onChange={(e) => update('flush_interval_ms', Number(e.target.value))}
                    disabled={!config.remote_enabled}
                  />
                </Grid>

                <Grid size={{ xs: 12 }}>
                  <Stack direction="row" spacing={1} alignItems="center">
                    <Button
                      variant="outlined"
                      size="small"
                      startIcon={testing ? <CircularProgress size={16} /> : <Send />}
                      onClick={() => void handleTest()}
                      disabled={testing || !config.remote_enabled}
                    >
                      测试上报
                    </Button>
                    <Button
                      variant="contained"
                      size="small"
                      startIcon={saving ? <CircularProgress size={16} /> : <Save />}
                      onClick={() => void handleSave()}
                      disabled={saving}
                    >
                      保存配置
                    </Button>
                  </Stack>
                </Grid>
                {testMessage && (
                  <Grid size={{ xs: 12 }}>
                    <Alert severity={testMessage.success ? 'success' : 'error'}>
                      {testMessage.message}
                    </Alert>
                  </Grid>
                )}

                <Grid size={{ xs: 12 }}>
                  <Divider />
                </Grid>

                {/* 现场查看 */}
                <Grid size={{ xs: 12 }}>
                  <FormControlLabel
                    control={
                      <Switch
                        checked={config.viewer_enabled}
                        onChange={(_, v) => update('viewer_enabled', v)}
                      />
                    }
                    label="现场查看 / 导出（方向B：SSE 实时推流 + 下载）"
                  />
                </Grid>
                <Grid size={{ xs: 12, sm: 4 }}>
                  <TextField
                    select
                    fullWidth
                    size="small"
                    label="查看级别"
                    value={config.viewer_level}
                    onChange={(e) => update('viewer_level', e.target.value as LogLevel)}
                    disabled={!config.viewer_enabled}
                  >
                    {LOG_LEVEL_OPTIONS.map((l) => (
                      <MenuItem key={l} value={l}>
                        {l}
                      </MenuItem>
                    ))}
                  </TextField>
                </Grid>
                <Grid size={{ xs: 12, sm: 4 }}>
                  <Tooltip title="环形缓冲条数上限，同时受 1 MiB 字节硬顶约束（保证设备内存 ≤ 2MB）">
                    <TextField
                      fullWidth
                      size="small"
                      type="number"
                      label="缓冲容量 (100–10000 条)"
                      inputProps={{ min: 100, max: 10000, step: 100 }}
                      value={config.buffer_capacity}
                      onChange={(e) => update('buffer_capacity', Number(e.target.value))}
                    />
                  </Tooltip>
                </Grid>
                <Grid size={{ xs: 12, sm: 4 }}>
                  <Alert severity="info" sx={{ py: 0.5 }}>
                    缓冲约占 <strong>{bufferMemKB.toFixed(0)} KB</strong>（峰值总占用 ≤ 2 MB）
                  </Alert>
                </Grid>
                <Grid size={{ xs: 12 }}>
                  <Stack direction="row" spacing={1}>
                    <Chip size="small" label={`缓冲溢出丢弃: ${droppedOverflow}`} />
                    <Chip size="small" label={`远程积压丢弃: ${droppedRemote}`} />
                  </Stack>
                </Grid>
              </Grid>
            </CardContent>
          </Card>

          {/* ===== 实时查看器 ===== */}
          <Card>
            <CardHeader
              title="实时日志"
              action={
                <Stack direction="row" spacing={1} alignItems="center">
                  <ToggleButtonGroup
                    size="small"
                    value={minLevel}
                    exclusive
                    onChange={(_, v: LogLevel | null) => v && setMinLevel(v)}
                  >
                    {LOG_LEVEL_OPTIONS.map((l) => (
                      <ToggleButton key={l} value={l} sx={{ px: 1, textTransform: 'lowercase' }}>
                        {l}
                      </ToggleButton>
                    ))}
                  </ToggleButtonGroup>
                  <Tooltip title={paused ? '继续' : '暂停'}>
                    <IconButton size="small" onClick={() => setPaused((p) => !p)}>
                      {paused ? <PlayArrow /> : <Pause />}
                    </IconButton>
                  </Tooltip>
                  <Tooltip title="自动滚动">
                    <Switch
                      size="small"
                      checked={autoScroll}
                      onChange={(_, v) => setAutoScroll(v)}
                    />
                  </Tooltip>
                  <Tooltip title="清空（同时清设备缓冲）">
                    <IconButton size="small" onClick={() => void handleClear()}>
                      <Delete />
                    </IconButton>
                  </Tooltip>
                  <Tooltip title="导出为 .log 文本">
                    <IconButton size="small" onClick={() => void handleExport('text')}>
                      <Download />
                    </IconButton>
                  </Tooltip>
                  <Tooltip title="导出为 JSON">
                    <IconButton size="small" onClick={() => void handleExport('json')}>
                      <Download />
                    </IconButton>
                  </Tooltip>
                </Stack>
              }
            />
            <CardContent>
              {!config.viewer_enabled ? (
                <Alert severity="warning">现场查看已关闭，请在上方开启后查看实时日志。</Alert>
              ) : (
                <Box
                  ref={viewerRef}
                  sx={{
                    height: 480,
                    overflow: 'auto',
                    bgcolor: 'background.default',
                    border: 1,
                    borderColor: 'divider',
                    borderRadius: 1,
                    p: 1,
                    fontFamily: 'monospace',
                    fontSize: 12,
                    lineHeight: 1.5,
                  }}
                >
                  {visibleEntries.length === 0 ? (
                    <Typography variant="body2" sx={{ color: 'text.secondary', p: 2 }}>
                      等待日志流…（级别 ≥ {minLevel}）
                    </Typography>
                  ) : (
                    visibleEntries.map((e, idx) => (
                      <Box key={idx} sx={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
                        <Chip
                          size="small"
                          color={LEVEL_COLOR[e.level] ?? 'default'}
                          label={e.level}
                          sx={{ mr: 1, height: 18, fontSize: 10 }}
                        />
                        <span style={{ color: 'text.secondary' }}>{e.ts}</span>{' '}
                        <span style={{ color: 'text.secondary' }}>{e.target}:</span>{' '}
                        <span>{e.message}</span>
                        {e.fields && <span style={{ color: 'text.secondary' }}>  {e.fields}</span>}
                      </Box>
                    ))
                  )}
                </Box>
              )}
            </CardContent>
          </Card>
        </>
      )}

      <ErrorSnackbar error={error} onClose={() => setError(null)} />
      <SuccessSnackbar message={success} onClose={() => setSuccess(null)} />
    </Box>
  )
}

function SuccessSnackbar({
  message,
  onClose,
}: {
  message: string | null
  onClose: () => void
}) {
  if (!message) return null
  return (
    <Box
      sx={{
        position: 'fixed',
        bottom: 24,
        left: '50%',
        transform: 'translateX(-50%)',
        zIndex: 2000,
      }}
    >
      <Alert severity="success" onClose={onClose} variant="filled">
        {message}
      </Alert>
    </Box>
  )
}
