import { useCallback, useEffect, useState, type ChangeEvent } from 'react'
import {
  Alert,
  AlertTitle,
  Box,
  Button,
  Card,
  CardContent,
  Chip,
  CircularProgress,
  Divider,
  FormControlLabel,
  Paper,
  Snackbar,
  Stack,
  Switch,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableRow,
  TextField,
  Typography,
  useMediaQuery,
  useTheme,
} from '@mui/material'
import {
  CheckCircle,
  FactCheck,
  Refresh,
  RocketLaunch,
  Save,
  WarningAmber,
} from '@mui/icons-material'
import { api } from '../api'
import ErrorSnackbar from '../components/ErrorSnackbar'

const FORMAT_CHECK_ENABLED_KEY = 'init-script-format-check-enabled'

interface ScriptNotice {
  title: string
  description: string
  severity: 'info' | 'warning'
}

interface SensitiveCommandRule {
  label: string
  reason: string
  pattern: RegExp
}

interface SensitiveCommandMatch {
  lineNumber: number
  label: string
  reason: string
  line: string
}

const SENSITIVE_COMMAND_RULES: SensitiveCommandRule[] = [
  { label: 'rm -rf', reason: '可能删除系统文件或持久化数据', pattern: /\brm\s+-rf\b/i },
  { label: 'dd', reason: '可能直接覆盖块设备或镜像', pattern: /\bdd\b/i },
  { label: 'mkfs', reason: '可能格式化分区或存储设备', pattern: /\bmkfs(?:\.[\w-]+)?\b/i },
  { label: 'reboot', reason: '会触发设备重启', pattern: /\breboot\b/i },
  { label: 'poweroff', reason: '会触发设备关机', pattern: /\bpoweroff\b/i },
  { label: 'halt', reason: '会停止系统服务或关机', pattern: /\bhalt\b/i },
  { label: 'kill', reason: '可能终止关键进程', pattern: /\bkill(?:all)?\b/i },
  { label: 'iptables', reason: '会修改防火墙规则', pattern: /\biptables\b/i },
  { label: 'ifconfig down', reason: '可能直接关闭网络接口', pattern: /\bifconfig\b.*\bdown\b/i },
  { label: 'ip link set down', reason: '可能直接关闭网络接口', pattern: /\bip\s+link\s+set\b.*\bdown\b/i },
  { label: 'mount remount', reason: '会修改文件系统挂载状态', pattern: /\bmount\b.*\bremount\b/i },
  { label: 'umount', reason: '可能卸载正在使用的文件系统', pattern: /\bumount\b/i },
]

function readStoredBoolean(key: string): boolean | null {
  if (typeof window === 'undefined') {
    return null
  }

  const value = localStorage.getItem(key)
  if (value === 'true') return true
  if (value === 'false') return false
  return null
}

function formatLineNumbers(lineNumbers: number[]): string {
  const preview = lineNumbers.slice(0, 5).join(', ')
  return lineNumbers.length > 5 ? `${preview} 等 ${lineNumbers.length} 行` : preview
}

function getCheckableLines(script: string): string[] {
  return script
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.startsWith('#'))
}

function countKeywordMatches(lines: string[], keyword: string): number {
  const pattern = new RegExp(`\\b${keyword}\\b`, 'g')

  return lines.reduce((count, line) => {
    return count + (line.match(pattern)?.length ?? 0)
  }, 0)
}

function countUnescapedQuotes(script: string, quote: '"' | "'"): number {
  let count = 0

  for (let index = 0; index < script.length; index += 1) {
    if (script[index] !== quote) {
      continue
    }

    let slashCount = 0
    for (let cursor = index - 1; cursor >= 0 && script[cursor] === '\\'; cursor -= 1) {
      slashCount += 1
    }

    if (slashCount % 2 === 0) {
      count += 1
    }
  }

  return count
}

function analyzeScriptFormat(script: string): ScriptNotice[] {
  const notices: ScriptNotice[] = []

  if (!script.trim()) {
    return notices
  }

  if (script.includes('\r')) {
    notices.push({
      title: '检测到 Windows 换行符',
      description: '当前脚本包含 CRLF 换行，设备端 shell 更适合使用 LF 换行。',
      severity: 'warning',
    })
  }

  const lines = script.split('\n')
  const checkableLines = getCheckableLines(script)

  const trailingWhitespaceLines = lines.reduce<number[]>((result, line, index) => {
    if (/[ \t]+$/.test(line)) {
      result.push(index + 1)
    }
    return result
  }, [])

  if (trailingWhitespaceLines.length > 0) {
    notices.push({
      title: '检测到行尾空格',
      description: `第 ${formatLineNumbers(trailingWhitespaceLines)} 存在多余空格或制表符。`,
      severity: 'info',
    })
  }

  if (countUnescapedQuotes(script, '"') % 2 !== 0) {
    notices.push({
      title: '双引号可能未闭合',
      description: '检测到未配对的双引号，建议检查 echo、变量拼接和命令替换语句。',
      severity: 'warning',
    })
  }

  if (countUnescapedQuotes(script, "'") % 2 !== 0) {
    notices.push({
      title: '单引号可能未闭合',
      description: '检测到未配对的单引号，建议检查字符串字面量是否完整。',
      severity: 'warning',
    })
  }

  const blockPairs: Array<[string, string, string]> = [
    ['if', 'fi', 'if / fi'],
    ['case', 'esac', 'case / esac'],
    ['do', 'done', 'do / done'],
  ]

  for (const [startKeyword, endKeyword, label] of blockPairs) {
    const startCount = countKeywordMatches(checkableLines, startKeyword)
    const endCount = countKeywordMatches(checkableLines, endKeyword)

    if (startCount !== endCount) {
      notices.push({
        title: `${label} 数量不平衡`,
        description: `${startKeyword} 共 ${startCount} 个，${endKeyword} 共 ${endCount} 个，建议检查控制流是否完整。`,
        severity: 'warning',
      })
    }
  }

  const continuationLines = lines.reduce<number[]>((result, line, index) => {
    const trimmed = line.trim()
    if (!trimmed || trimmed.startsWith('#')) {
      return result
    }

    if (/(?:&&|\|\||\||\\)$/.test(trimmed)) {
      result.push(index + 1)
    }

    return result
  }, [])

  if (continuationLines.length > 0) {
    notices.push({
      title: '存在续行命令',
      description: `第 ${formatLineNumbers(continuationLines)} 以续行符结束，请确认下一行命令完整存在。`,
      severity: 'info',
    })
  }

  if (!script.endsWith('\n')) {
    notices.push({
      title: '建议保留结尾换行',
      description: '为避免追加内容时拼接到同一行，建议脚本以换行结尾。',
      severity: 'info',
    })
  }

  return notices
}

function analyzeSensitiveCommands(script: string): SensitiveCommandMatch[] {
  const matches: SensitiveCommandMatch[] = []

  script.split('\n').forEach((rawLine, index) => {
    const trimmed = rawLine.trim()

    if (!trimmed || trimmed.startsWith('#')) {
      return
    }

    for (const rule of SENSITIVE_COMMAND_RULES) {
      if (rule.pattern.test(trimmed)) {
        matches.push({
          lineNumber: index + 1,
          label: rule.label,
          reason: rule.reason,
          line: rawLine,
        })
      }
    }
  })

  return matches
}

function buildSensitiveLineMap(matches: SensitiveCommandMatch[]): Record<number, SensitiveCommandMatch[]> {
  return matches.reduce<Record<number, SensitiveCommandMatch[]>>((result, match) => {
    if (!result[match.lineNumber]) {
      result[match.lineNumber] = []
    }
    result[match.lineNumber].push(match)
    return result
  }, {})
}

export default function InitScriptPage() {
  const theme = useTheme()
  const isMobile = useMediaQuery(theme.breakpoints.down('sm'))
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState<string | null>(null)
  const [script, setScript] = useState('')
  const [initPath, setInitPath] = useState('/home/root/init.sh')
  const [loaderPath, setLoaderPath] = useState('/home/root/loader.sh')
  const [loaderHooked, setLoaderHooked] = useState(false)
  const [formatCheckEnabled, setFormatCheckEnabled] = useState(() => {
    return readStoredBoolean(FORMAT_CHECK_ENABLED_KEY) ?? true
  })

  const formatNotices = formatCheckEnabled ? analyzeScriptFormat(script) : []
  const sensitiveMatches = analyzeSensitiveCommands(script)
  const sensitiveLineMap = buildSensitiveLineMap(sensitiveMatches)
  const scriptLines = script.split('\n')
  const cardContentSx = {
    p: { xs: 2, sm: 3 },
    '&:last-child': {
      pb: { xs: 2, sm: 3 },
    },
  }
  const startupRows = [
    { label: 'loader.sh 路径', value: loaderPath },
    { label: 'init.sh 路径', value: initPath },
    { label: '固定启动命令', value: 'sh /home/root/init.sh &' },
  ]

  const loadScript = useCallback(async () => {
    setLoading(true)
    setError(null)

    try {
      const response = await api.getInitScript()
      if (response.data) {
        setScript(response.data.script || '')
        setInitPath(response.data.init_path)
        setLoaderPath(response.data.loader_path)
        setLoaderHooked(response.data.loader_hooked)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void loadScript()
  }, [loadScript])

  useEffect(() => {
    if (typeof window === 'undefined') {
      return
    }

    localStorage.setItem(FORMAT_CHECK_ENABLED_KEY, formatCheckEnabled ? 'true' : 'false')
  }, [formatCheckEnabled])

  const handleSave = async () => {
    setSaving(true)
    setError(null)
    setSuccess(null)

    try {
      const response = await api.setInitScript(script)
      if (response.data) {
        setScript(response.data.script || '')
        setInitPath(response.data.init_path)
        setLoaderPath(response.data.loader_path)
        setLoaderHooked(response.data.loader_hooked)
      }
      setSuccess('init.sh 已保存，loader.sh 的启动入口已同步确认。')
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setSaving(false)
    }
  }

  if (loading) {
    return (
      <Box display="flex" justifyContent="center" alignItems="center" minHeight="60vh">
        <CircularProgress />
      </Box>
    )
  }

  return (
    <Box sx={{ width: '100%', maxWidth: 1120, mx: 'auto' }}>
      <Box mb={{ xs: 2.5, sm: 3 }}>
        <Typography
          variant="h4"
          component="h1"
          gutterBottom
          fontWeight={600}
          sx={{ fontSize: { xs: '2rem', sm: '2.25rem' }, lineHeight: 1.15 }}
        >
          开机脚本
        </Typography>
        <Typography variant="body2" color="text.secondary">
          这里编辑的是 <code>init.sh</code> 内容。保存时会确保 <code>loader.sh</code> 末尾保留
          {' '}
          <code>sh /home/root/init.sh &amp;</code>
          {' '}
          作为固定启动入口。
        </Typography>
      </Box>

      <ErrorSnackbar error={error} onClose={() => setError(null)} />
      {success && (
        <Snackbar
          open
          autoHideDuration={3000}
          onClose={() => setSuccess(null)}
          anchorOrigin={{ vertical: 'top', horizontal: 'center' }}
        >
          <Alert severity="success" variant="filled" onClose={() => setSuccess(null)}>
            {success}
          </Alert>
        </Snackbar>
      )}

      <Stack spacing={{ xs: 2, sm: 3 }}>
        <Card>
          <CardContent sx={cardContentSx}>
            <Box
              display="flex"
              justifyContent="space-between"
              alignItems={{ xs: 'flex-start', md: 'center' }}
              flexDirection={{ xs: 'column', md: 'row' }}
              gap={{ xs: 1.5, sm: 2 }}
              mb={{ xs: 2, sm: 2.5 }}
            >
              <Box sx={{ width: '100%' }}>
                <Typography variant="h6">脚本编辑</Typography>
                <Stack direction="row" spacing={0.75} useFlexGap flexWrap="wrap" mt={1}>
                  <Chip
                    label={loaderHooked ? 'loader.sh 已挂载 init.sh' : 'loader.sh 尚未挂载 init.sh'}
                    color={loaderHooked ? 'success' : 'warning'}
                    size="small"
                  />
                  <Chip
                    label={
                      formatCheckEnabled
                        ? `格式检查 ${formatNotices.length === 0 ? '通过' : `${formatNotices.length} 条提醒`}`
                        : '格式检查已关闭'
                    }
                    color={!formatCheckEnabled ? 'default' : formatNotices.length === 0 ? 'success' : 'warning'}
                    size="small"
                  />
                  <Chip
                    label={sensitiveMatches.length > 0 ? `敏感命令 ${sensitiveMatches.length} 处` : '未发现敏感命令'}
                    color={sensitiveMatches.length > 0 ? 'warning' : 'success'}
                    size="small"
                  />
                </Stack>
              </Box>
              <Stack
                direction={{ xs: 'column', sm: 'row' }}
                spacing={1.25}
                sx={{ width: { xs: '100%', md: 'auto' } }}
              >
                <Button
                  variant="contained"
                  onClick={() => void handleSave()}
                  disabled={saving}
                  startIcon={saving ? <CircularProgress size={20} color="inherit" /> : <Save />}
                  sx={{ width: { xs: '100%', sm: 'auto' } }}
                >
                  {saving ? '保存中...' : '保存脚本'}
                </Button>
                <Button
                  variant="outlined"
                  onClick={() => void loadScript()}
                  disabled={loading || saving}
                  startIcon={<Refresh />}
                  sx={{ width: { xs: '100%', sm: 'auto' } }}
                >
                  重新加载
                </Button>
                <Button
                  variant="text"
                  color="warning"
                  onClick={() => setScript('')}
                  disabled={saving}
                  sx={{ width: { xs: '100%', sm: 'auto' } }}
                >
                  清空编辑器
                </Button>
              </Stack>
            </Box>

            <Typography variant="body2" color="text.secondary" mb={2}>
              这里只编辑 <code>init.sh</code> 内容。保存时会确保 <code>loader.sh</code> 末尾保留
              {' '}
              <code>sh /home/root/init.sh &amp;</code>
              {' '}
              作为固定启动入口，格式检查开关会自动记住当前选择。
            </Typography>

            <TextField
              fullWidth
              multiline
              minRows={isMobile ? 14 : 18}
              label="init.sh 内容"
              value={script}
              onChange={(event: ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => setScript(event.target.value)}
              placeholder={'# 在这里写开机后要执行的 shell 命令\n# 示例:\n# echo "boot ok" > /tmp/boot.log'}
              spellCheck={false}
              InputProps={{
                sx: {
                  fontFamily: 'monospace',
                  alignItems: 'flex-start',
                  fontSize: { xs: '0.95rem', sm: '1rem' },
                },
              }}
            />
          </CardContent>
        </Card>

        <Box
          sx={{
            display: 'grid',
            gridTemplateColumns: { xs: '1fr', lg: 'minmax(0, 1.05fr) minmax(320px, 0.95fr)' },
            gap: 3,
          }}
        >
          <Card>
            <CardContent>
              <Box display="flex" alignItems="center" gap={1} mb={2}>
                <FactCheck color="primary" />
                <Typography variant="h6">检查选项</Typography>
              </Box>

              <FormControlLabel
                control={(
                  <Switch
                    checked={formatCheckEnabled}
                    onChange={(event: ChangeEvent<HTMLInputElement>) => setFormatCheckEnabled(event.target.checked)}
                  />
                )}
                label="启用简单格式检查"
              />

              <Typography variant="body2" color="text.secondary" mt={1}>
                默认开启，并自动记住当前开关状态。关闭后仍会保留敏感命令标记，但不再给出格式提醒。
              </Typography>
            </CardContent>
          </Card>

          <Card>
            <CardContent>
              <Box
                display="flex"
                justifyContent="space-between"
                alignItems={{ xs: 'flex-start', sm: 'center' }}
                flexDirection={{ xs: 'column', sm: 'row' }}
                gap={1.5}
                mb={2}
              >
                <Box display="flex" alignItems="center" gap={1}>
                  <RocketLaunch color="primary" />
                  <Typography variant="h6">启动入口</Typography>
                </Box>
                <Chip
                  label={loaderHooked ? 'loader.sh 已挂载 init.sh' : 'loader.sh 尚未挂载 init.sh'}
                  color={loaderHooked ? 'success' : 'warning'}
                  size="small"
                />
              </Box>

              {isMobile ? (
                <Stack spacing={1.25}>
                  {startupRows.map((row) => (
                    <Box
                      key={row.label}
                      sx={{
                        p: 1.5,
                        border: 1,
                        borderColor: 'divider',
                        borderRadius: 1.5,
                        minWidth: 0,
                      }}
                    >
                      <Typography variant="caption" color="text.secondary">
                        {row.label}
                      </Typography>
                      <Typography
                        variant="body2"
                        sx={{
                          mt: 0.5,
                          fontFamily: 'monospace',
                          wordBreak: 'break-all',
                        }}
                      >
                        {row.value}
                      </Typography>
                    </Box>
                  ))}
                </Stack>
              ) : (
              <TableContainer>
                <Table size="small">
                  <TableBody>
                    <TableRow>
                      <TableCell sx={{ width: 160 }}>loader.sh 路径</TableCell>
                      <TableCell sx={{ fontFamily: 'monospace' }}>{loaderPath}</TableCell>
                    </TableRow>
                    <TableRow>
                      <TableCell>init.sh 路径</TableCell>
                      <TableCell sx={{ fontFamily: 'monospace' }}>{initPath}</TableCell>
                    </TableRow>
                    <TableRow>
                      <TableCell>固定启动命令</TableCell>
                      <TableCell sx={{ fontFamily: 'monospace' }}>sh /home/root/init.sh &amp;</TableCell>
                    </TableRow>
                  </TableBody>
                </Table>
              </TableContainer>
              )}
            </CardContent>
          </Card>
        </Box>

        <Alert severity={sensitiveMatches.length > 0 ? 'warning' : 'info'}>
          <AlertTitle>说明</AlertTitle>
          页面只维护 <code>init.sh</code> 文件，不会覆盖 <code>loader.sh</code> 原有启动逻辑。
          如果脚本留空，设备仍会保留启动入口。敏感命令只做标记提醒，不会阻止保存。
        </Alert>

        <Card>
          <CardContent>
            <Box display="flex" alignItems="center" gap={1} mb={2}>
              <FactCheck color="primary" />
              <Typography variant="h6">格式检查结果</Typography>
            </Box>

            {!formatCheckEnabled && (
              <Alert severity="info">
                当前已关闭格式检查。重新打开后会继续对控制流、引号和换行做基础提醒。
              </Alert>
            )}

            {formatCheckEnabled && formatNotices.length === 0 && (
              <Alert severity="success" icon={<CheckCircle fontSize="inherit" />}>
                没有发现明显的格式问题。
              </Alert>
            )}

            {formatCheckEnabled && formatNotices.length > 0 && (
              <Stack spacing={1.5}>
                {formatNotices.map((notice) => (
                  <Alert key={`${notice.title}-${notice.description}`} severity={notice.severity}>
                    <AlertTitle>{notice.title}</AlertTitle>
                    {notice.description}
                  </Alert>
                ))}
              </Stack>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardContent>
            <Box display="flex" alignItems="center" gap={1} mb={2}>
              <WarningAmber color={sensitiveMatches.length > 0 ? 'warning' : 'disabled'} />
              <Typography variant="h6">命令预览与标记</Typography>
            </Box>

            {!script.trim() && (
              <Alert severity="info">
                当前没有额外启动命令。
              </Alert>
            )}

            {script.trim() && sensitiveMatches.length === 0 && (
              <Alert severity="success" icon={<CheckCircle fontSize="inherit" />}>
                预览中未发现需要特别注意的敏感命令。
              </Alert>
            )}

            {script.trim() && sensitiveMatches.length > 0 && (
              <Alert severity="warning" sx={{ mb: 2 }}>
                <AlertTitle>已标记敏感命令</AlertTitle>
                {sensitiveMatches.map((match) => (
                  <Typography
                    key={`${match.lineNumber}-${match.label}-${match.line}`}
                    variant="body2"
                    sx={{ '& + &': { mt: 0.5 } }}
                  >
                    第 {match.lineNumber} 行: <strong>{match.label}</strong>
                    {' '}
                    {match.reason}
                  </Typography>
                ))}
              </Alert>
            )}

            {script.trim() && (
              <>
                <Divider sx={{ mb: 2 }} />
                <Typography variant="subtitle2" gutterBottom>
                  脚本预览
                </Typography>
                <Paper variant="outlined" sx={{ overflow: 'hidden' }}>
                  {scriptLines.map((line: string, index: number) => {
                    const lineNumber = index + 1
                    const matches = sensitiveLineMap[lineNumber] ?? []
                    const isSensitive = matches.length > 0

                    return (
                      <Box
                        key={`${lineNumber}-${line}`}
                        sx={{
                          display: 'grid',
                          gridTemplateColumns: isMobile ? '40px minmax(0, 1fr)' : '56px minmax(0, 1fr)',
                          gap: isMobile ? 1 : 2,
                          px: isMobile ? 1.25 : 2,
                          py: 0.75,
                          borderBottom: index === scriptLines.length - 1 ? 'none' : 1,
                          borderColor: 'divider',
                          backgroundColor: isSensitive ? 'rgba(211, 47, 47, 0.08)' : 'transparent',
                        }}
                      >
                        <Typography
                          variant="body2"
                          color={isSensitive ? 'error.main' : 'text.secondary'}
                          sx={{ fontFamily: 'monospace' }}
                        >
                          {lineNumber}
                        </Typography>
                        <Box sx={{ minWidth: 0 }}>
                          <Box
                            component="pre"
                            sx={{
                              m: 0,
                              fontSize: '0.875rem',
                              fontFamily: 'monospace',
                              whiteSpace: 'pre-wrap',
                              wordBreak: 'break-word',
                              color: isSensitive ? 'error.main' : 'text.primary',
                            }}
                          >
                            {line || ' '}
                          </Box>
                          {matches.length > 0 && (
                            <Stack direction="row" spacing={1} useFlexGap flexWrap="wrap" mt={1}>
                              {matches.map((match) => (
                                <Chip
                                  key={`${lineNumber}-${match.label}`}
                                  label={match.label}
                                  color="error"
                                  size="small"
                                  variant="outlined"
                                />
                              ))}
                            </Stack>
                          )}
                        </Box>
                      </Box>
                    )
                  })}
                </Paper>
              </>
            )}
          </CardContent>
        </Card>
      </Stack>
    </Box>
  )
}
