import {
  Article as LogsIcon,
  Dashboard as DashboardIcon,
  Devices as DevicesIcon,
  GitHub as GitHubIcon,
  Phone as PhoneIcon,
  RocketLaunch as InitScriptIcon,
  Settings as SettingsIcon,
  SignalCellularAlt as SignalIcon,
  Sms as SmsIcon,
  SystemUpdateAlt as OtaIcon,
  Terminal as TerminalIcon,
  WebAsset as WebTerminalIcon,
} from '@mui/icons-material'

export const navGroups = [
  { id: 'overview', label: '概览' },
  { id: 'network', label: '网络与 SIM' },
  { id: 'communication', label: '通信' },
  { id: 'system', label: '系统维护' },
  { id: 'tools', label: '调试工具' },
] as const

export type NavGroupId = (typeof navGroups)[number]['id']

export const appPages = [
  {
    id: 'dashboard',
    path: '/',
    label: '仪表盘',
    title: '仪表盘',
    subtitle: '查看设备运行状态、网络质量和高频控制项。',
    group: 'overview',
    icon: DashboardIcon,
  },
  {
    id: 'device',
    path: '/device',
    label: '设备信息',
    title: '设备信息',
    subtitle: '查看设备标识、SIM 卡状态和硬件参数。',
    group: 'network',
    icon: DevicesIcon,
  },
  {
    id: 'network',
    path: '/network',
    label: '网络状态',
    title: '网络状态',
    subtitle: '管理小区、频段、APN、运营商和网络接口。',
    group: 'network',
    icon: SignalIcon,
  },
  {
    id: 'phone',
    path: '/phone',
    label: '电话管理',
    title: '电话管理',
    subtitle: '拨号、查看通话记录并调整语音通话设置。',
    group: 'communication',
    icon: PhoneIcon,
  },
  {
    id: 'sms',
    path: '/sms',
    label: '短信管理',
    title: '短信管理',
    subtitle: '按对话查看、发送和清理短信记录。',
    group: 'communication',
    icon: SmsIcon,
  },
  {
    id: 'config',
    path: '/config',
    label: '系统配置',
    title: '系统配置',
    subtitle: '管理连接开关、USB 模式、通知转发和服务状态。',
    group: 'system',
    icon: SettingsIcon,
  },
  {
    id: 'initScript',
    path: '/init-script',
    label: '开机脚本',
    title: '开机脚本',
    subtitle: '编辑启动脚本，并检查格式和高风险命令。',
    group: 'system',
    icon: InitScriptIcon,
  },
  {
    id: 'ota',
    path: '/ota',
    label: 'OTA 更新',
    title: 'OTA 更新',
    subtitle: '上传、验证并应用系统更新包。',
    group: 'system',
    icon: OtaIcon,
  },
  {
    id: 'logs',
    path: '/logs',
    label: '日志',
    title: '日志',
    subtitle: '配置日志上报，实时查看并导出运行日志。',
    group: 'system',
    icon: LogsIcon,
  },
  {
    id: 'atConsole',
    path: '/at-console',
    label: 'AT 控制台',
    title: 'AT 控制台',
    subtitle: '发送 AT 指令并查看设备返回结果。',
    group: 'tools',
    icon: TerminalIcon,
  },
  {
    id: 'terminal',
    path: '/terminal',
    label: 'Web 终端',
    title: 'Web 终端',
    subtitle: '通过 ttyd 访问设备命令行。',
    group: 'tools',
    icon: WebTerminalIcon,
  },
] as const

export type AppPage = (typeof appPages)[number]
export type AppPageId = AppPage['id']

export function getPageById(pageId: AppPageId): AppPage {
  const page = appPages.find((item) => item.id === pageId)
  if (!page) {
    throw new Error(`未知页面: ${pageId}`)
  }
  return page
}

export function getGroupById(groupId: NavGroupId) {
  return navGroups.find((group) => group.id === groupId)
}

export function getPageByPath(pathname: string): AppPage | undefined {
  const normalizedPath = pathname.replace(/\/+$/, '') || '/'
  return appPages.find((page) => page.path === normalizedPath)
}

export const githubLink = {
  href: 'https://github.com/rbetree/project-cpe',
  label: 'rbetree/project-cpe',
  icon: GitHubIcon,
}
