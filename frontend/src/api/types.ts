/*
 * @Author: 1orz cloudorzi@gmail.com
 * @Date: 2025-12-13 12:41:48
 * @LastEditors: 1orz cloudorzi@gmail.com
 * @LastEditTime: 2025-12-13 12:42:58
 * @FilePath: /udx710-backend/frontend/src/api/types.ts
 * @Description: 
 * 
 * Copyright (c) 2025 by 1orz, All Rights Reserved. 
 */
// API 响应统一格式
export interface ApiResponse<T> {
  status: string
  message: string
  data?: T
}

// 设备信息（来自 D-Bus Modem 接口）
export interface DeviceInfo {
  imei: string // IMEI 设备序列号
  manufacturer: string // 制造商
  model: string // 型号
  revision?: string // 固件版本
  online: boolean // 是否在线（射频开启）
  powered: boolean // 是否上电
}

// SIM 卡信息（整合所有 SIM 相关信息）
export interface SimInfo {
  present: boolean // SIM 卡是否存在
  iccid: string // ICCID
  imsi: string // IMSI
  phone_numbers: string[] // 手机号码列表
  sms_center: string // 短信中心号码
  mcc: string // 移动国家代码
  mnc: string // 移动网络代码
  pin_required: string // PIN 状态
  preferred_languages: string[] // 首选语言列表
}

// 网络信息
export interface NetworkInfo {
  operator_name: string
  registration_status: string
  technology_preference: string
  signal_strength: number
  mcc?: string // 移动国家代码
  mnc?: string // 移动网络代码
}

// 服务小区
export interface ServingCell {
  tech: string
  cell_id: number
  tac: number
}

// 小区信息
export interface CellInfo {
  is_serving?: boolean // 是否为主服务小区
  type?: string // 兼容旧版本
  tech?: string // 网络制式
  band?: string // 频段
  pci?: string | number // 物理小区标识
  earfcn?: number
  /// 绝对频点号（兼容后端新字段）
  arfcn?: string | number
  // 信号强度字段（后端返回的是字符串形式的原始值×100）
  rsrp?: string | number
  rsrq?: string | number
  rssi?: string | number
  sinr?: string | number
  // NR 5G 字段
  nrarfcn?: number
  ssb_rsrp?: string | number
  ssb_rsrq?: string | number
  ssb_sinr?: string | number
}

// 小区列表响应
export interface CellsResponse {
  serving_cell: ServingCell
  cells: CellInfo[]
}

// QoS 信息
export interface QosInfo {
  qci: number
  dl_speed: number
  ul_speed: number
}

// 温度传感器
export interface ThermalZone {
  zone: string
  type: string
  temperature: number
}

// 数据连接状态
export interface DataConnectionStatus {
  active: boolean
}

// 漫游状态响应
export interface RoamingResponse {
  roaming_allowed: boolean  // 是否允许漫游数据
  is_roaming: boolean       // 当前是否处于漫游状态
}

// 漫游设置请求
export interface RoamingRequest {
  allowed: boolean
}

// USB 模式
export interface UsbModeResponse {
  current_mode: number | null // 当前硬件实际运行的模式（从 configfs 读取）
  current_mode_name: string
  permanent_mode?: number | null // 永久配置（从 /mnt/data/mode.cfg）
  temporary_mode?: number | null // 临时配置（从 /mnt/data/mode_tmp.cfg）
  needs_reboot: boolean // 是否需要重启
  read_mode: string
}

// AT 指令请求
export interface AtCommandRequest {
  cmd: string
}

// USB 模式设置请求
export interface SetUsbModeRequest {
  mode: number // 1=CDC-NCM, 2=CDC-ECM, 3=RNDIS
  permanent?: boolean // true=永久模式, false=临时模式（默认 false）
}

// 系统重启请求
export interface SystemRebootRequest {
  delay_seconds?: number // 延迟秒数，默认为 3
}

// 数据连接请求
export interface DataConnectionRequest {
  active: boolean
}

// 飞行模式请求
export interface AirplaneModeRequest {
  enabled: boolean
}

// 飞行模式响应
export interface AirplaneModeResponse {
  enabled: boolean
  powered: boolean
  online: boolean
}

// 网络速度信息
export interface NetworkSpeed {
  interface: string
  rx_bytes_per_sec: number
  tx_bytes_per_sec: number
  total_rx_bytes: number
  total_tx_bytes: number
}

// 网络速度响应
export interface NetworkSpeedResponse {
  interfaces: NetworkSpeed[]
  interval_seconds: number
}

// 内存信息
export interface MemoryInfo {
  total_bytes: number
  available_bytes: number
  used_bytes: number
  used_percent: number
  cached_bytes: number
  buffers_bytes: number
}

// 运行时间信息
export interface UptimeInfo {
  uptime_seconds: number
  idle_seconds: number
  uptime_formatted: string
}

// 系统信息（uname）
export interface SystemInfo {
  sysname: string
  nodename: string
  release: string
  version: string
  machine: string
  domainname?: string
  full_info: string
}

// 综合系统状态
export interface SystemStatsResponse {
  network_speed: NetworkSpeedResponse
  memory: MemoryInfo
  disk: DiskInfo[] // 磁盘信息
  cpu_load: CpuLoadInfo
  uptime: UptimeInfo
  system_info: SystemInfo
  temperature: ThermalZone[] // 温度传感器数据
  usb_mode: UsbModeResponse // USB 模式信息
}

// 磁盘/分区信息
export interface DiskInfo {
  mount_point: string
  fs_type: string
  total_bytes: number
  used_bytes: number
  available_bytes: number
  used_percent: number
}

// CPU 负载信息
export interface CpuLoadInfo {
  load_1min: number
  load_5min: number
  load_15min: number
  core_count: number
  load_percent: number
}

// CPU 核心信息
export interface CpuCore {
  processor: number
  bogomips: string
  features: string[]
  implementer: string
  architecture: string
  variant: string
  part: string
  revision: string
}

// CPU 信息
export interface CpuInfo {
  core_count: number
  cores: CpuCore[]
  hardware: string
  serial: string
  model_name: string
}

// 基站定位参数
export interface CellLocationInfo {
  mcc: string // 移动国家代码
  mnc: string // 移动网络代码
  lac: number // 位置区码/跟踪区码
  cid: number // 小区ID
  signal_strength: number // 信号强度（RSRP，单位：dBm）
  radio_type: string // 网络制式
  arfcn?: number // 绝对频点号
  pci?: number // 物理小区标识
  rsrq?: number // 参考信号接收质量（单位：dB）
  sinr?: number // 信噪比（单位：dB）
}

// 基站定位信息响应
export interface CellLocationResponse {
  available: boolean // 是否可用
  cell_info?: CellLocationInfo // 主服务小区定位参数
  neighbor_cells: CellLocationInfo[] // 邻区定位参数列表
  usage_hint: string // 使用建议
  // 兼容字段：合并主小区和邻区为一个列表
  cells?: CellLocationInfo[]
}

// IP地址信息
export interface IpAddress {
  address: string // IP地址
  prefix_len: number // 前缀长度（子网掩码位数）
  ip_type: string // IP类型：ipv4 或 ipv6
  scope: string // 地址范围：private（内网）, public（公网）, loopback（回环）, link-local（链路本地）
}

// 网络接口详细信息
export interface NetworkInterfaceInfo {
  name: string // 接口名称（如 eth0, wlan0, usb0）
  status: string // 接口状态：up, down
  mac_address?: string // MAC地址
  mtu: number // MTU（最大传输单元）
  ip_addresses: IpAddress[] // IP地址列表（IPv4和IPv6）
  rx_bytes: number // 接收字节数
  tx_bytes: number // 发送字节数
  rx_packets: number // 接收包数
  tx_packets: number // 发送包数
  rx_errors: number // 接收错误数
  tx_errors: number // 发送错误数
}

// 网络接口列表响应
export interface NetworkInterfacesResponse {
  interfaces: NetworkInterfaceInfo[] // 网络接口列表
  total_count: number // 接口总数
}

// 射频模式类型
export type RadioMode = 'auto' | 'lte' | 'nr'

// 射频模式响应
export interface RadioModeResponse {
  mode: string // 当前模式: auto | lte | nr | unknown
  technology_preference: string // ofono 原始 TechnologyPreference 值
}

// 射频模式请求
export interface RadioModeRequest {
  mode: RadioMode // auto: 4G/5G 自动, lte: 仅 4G, nr: 仅 5G
}

// 频段锁定状态
export interface BandLockStatus {
  locked: boolean // 是否已锁定频段
  lte_fdd_bands: number[] // LTE FDD 频段列表 (如 [1, 3, 8])
  lte_tdd_bands: number[] // LTE TDD 频段列表 (如 [38, 40, 41])
  nr_fdd_bands: number[] // NR FDD 频段列表 (如 [1, 28])
  nr_tdd_bands: number[] // NR TDD 频段列表 (如 [41, 77, 78, 79])
  raw_response?: string // 原始 AT 响应（可选）
}

// 频段锁定请求
export interface BandLockRequest {
  lte_fdd_bands: number[] // LTE FDD 频段列表
  lte_tdd_bands: number[] // LTE TDD 频段列表
  nr_fdd_bands: number[] // NR FDD 频段列表
  nr_tdd_bands: number[] // NR TDD 频段列表
}

// ========== 小区锁定类型 ==========

// 单个 RAT 的小区锁定状态
export interface CellLockRatStatus {
  rat: number // RAT 类型 (12=LTE, 16=NR)
  rat_name: string // RAT 名称
  enabled: boolean // 是否启用锁定
  lock_type: number // 锁定类型
  pci: number | null // 锁定的 PCI
  arfcn: number | null // 锁定的 ARFCN
}

// 小区锁定状态响应
export interface CellLockStatusResponse {
  rat_status: CellLockRatStatus[] // 各 RAT 的锁定状态
  any_locked: boolean // 是否有任何锁定生效
}

// 小区锁定请求
export interface CellLockRequest {
  rat: number // RAT 类型 (12=LTE, 16=NR)
  enable: boolean // 是否启用锁定
  lock_type?: number // 锁定类型 (保留字段)
  pci?: number // PCI（物理小区标识）
  arfcn?: number // ARFCN（绝对频点号）
}

// 小区锁定结果
export interface CellLockResult {
  locked?: boolean
  tech?: string
  arfcn?: number
  pci?: number
  success?: boolean
  steps?: string[]
  raw_response?: string
}

// ========== 电话相关类型 ==========

// 通话信息
export interface CallInfo {
  path: string // D-Bus 对象路径
  phone_number: string // 电话号码
  state: string // 通话状态: active, dialing, alerting, incoming, held
  direction: string // 通话方向: incoming | outgoing
  start_time?: string // 开始时间
}

// 通话列表响应
export interface CallListResponse {
  calls: CallInfo[] // 当前通话列表
}

// 拨打电话请求
export interface MakeCallRequest {
  phone_number: string // 目标电话号码
}

// 挂断电话请求
export interface HangupCallRequest {
  path: string // 通话路径
}

// ========== 短信相关类型 ==========

// 短信记录
export interface SmsMessage {
  id: number // 短信 ID
  direction: string // 方向: incoming | outgoing
  phone_number: string // 发件人或收件人
  content: string // 短信内容
  timestamp: string // ISO 8601 时间
  status: string // 状态: pending, sent, failed, received
  pdu?: string // 原始 PDU（可选）
}

// 发送短信请求
export interface SendSmsRequest {
  phone_number: string // 目标电话号码
  content: string // 短信内容
}

// 短信列表请求
export interface SmsListRequest {
  limit?: number // 每页数量（默认 50）
  offset?: number // 偏移量（默认 0）
}

// 短信对话请求
export interface SmsConversationRequest {
  phone_number: string // 电话号码
  limit?: number // 最多返回条数（默认 50）
}

// 短信统计
export interface SmsStats {
  total: number // 总短信数
  incoming: number // 接收短信数
  outgoing: number // 发送短信数
}

// ========== 新增功能类型 ==========

// IMEISV（软件版本号）
export interface ImeisvResponse {
  software_version_number: string // 软件版本号
}

// 信号强度详细信息
export interface SignalStrengthResponse {
  strength: number // 信号强度（0-100 或负数 dBm）
}

// NITZ 网络时间
export interface NitzTimeResponse {
  time_string: string // 网络时间字符串
  available: boolean // 是否可用
}

// IMS（VoLTE）状态
export interface ImsStatusResponse {
  registered: boolean // 是否已注册到 IMS
  voice_capable: boolean // 是否支持语音通话
  sms_capable: boolean // 是否支持短信
}

// 通话音量
export interface CallVolumeResponse {
  speaker_volume: number // 扬声器音量（0-100）
  microphone_volume: number // 麦克风音量（0-100）
  muted: boolean // 是否静音
}

export interface SetCallVolumeRequest {
  speaker_volume?: number // 扬声器音量（可选）
  microphone_volume?: number // 麦克风音量（可选）
  muted?: boolean // 是否静音（可选）
}

// 语音留言状态
export interface VoicemailStatusResponse {
  waiting: boolean // 是否有语音留言等待
  message_count: number // 留言数量
  mailbox_number: string // 语音信箱号码
}

// 运营商信息
export interface OperatorInfo {
  path: string // D-Bus 对象路径
  name: string // 运营商名称
  status: string // 状态：available, current, forbidden
  mcc: string // 移动国家代码
  mnc: string // 移动网络代码
  technologies: string[] // 支持的技术（如 ["LTE", "NR"]）
}

export interface OperatorListResponse {
  operators: OperatorInfo[] // 运营商列表
}

export interface ManualRegisterRequest {
  mccmnc: string // MCCMNC（如 "46001"）
}

// 呼叫转移
export interface CallForwardingResponse {
  voice_unconditional: string // 无条件转移号码
  voice_busy: string // 占线时转移号码
  voice_no_reply: string // 无应答时转移号码
  voice_no_reply_timeout: number // 无应答超时时间（秒）
  voice_not_reachable: string // 不可达时转移号码
  forwarding_flag_on_sim: boolean // SIM 卡上的转移标志
}

export interface SetCallForwardingRequest {
  forward_type: string // 转移类型：unconditional, busy, noreply, notreachable
  number: string // 目标号码（空字符串表示禁用）
  timeout?: number // 无应答超时（仅 noreply 类型需要）
}

// 通话设置
export interface CallSettingsResponse {
  calling_line_presentation: string // 主叫号码显示
  calling_name_presentation: string // 主叫姓名显示
  connected_line_presentation: string // 被叫号码显示
  connected_line_restriction: string // 被叫号码限制
  called_line_presentation: string // 已拨号码显示
  calling_line_restriction: string // 主叫号码限制
  hide_caller_id: string // 隐藏来电显示
  voice_call_waiting: string // 呼叫等待
}

export interface SetCallSettingRequest {
  property: string // 设置项：HideCallerId, VoiceCallWaiting
  value: string // 值：default/enabled/disabled
}

// ========== SIM 卡槽类型 ==========

// SIM 卡槽信息
export interface SimSlotResponse {
  active_slot: number // 当前激活的卡槽（1 或 2）
  raw_value: string // 原始值
}

// 切换 SIM 卡槽请求
export interface SwitchSimSlotRequest {
  slot: number // 目标卡槽（1 或 2）
}

// USB 热切换请求
export interface UsbAdvanceRequest {
  mode: number // USB 模式
}

// ========== APN 管理类型 ==========

// APN Context 信息
export interface ApnContext {
  path: string           // D-Bus 路径 (如 /ril_0/context2)
  name: string           // 名称
  active: boolean        // 是否激活
  apn: string            // APN 名称 (如 cbnet, cmnet)
  protocol: string       // 协议: ip/ipv6/dual
  username: string       // 用户名
  password: string       // 密码
  auth_method: string    // 认证方式: none/pap/chap
  context_type: string   // 类型: internet/mms/ims
}

// APN 列表响应
export interface ApnListResponse {
  contexts: ApnContext[]
}

// 设置 APN 请求
export interface SetApnRequest {
  context_path: string   // 要修改的 context 路径
  apn?: string           // APN 名称
  protocol?: string      // 协议: ip/ipv6/dual
  username?: string      // 用户名
  password?: string      // 密码
  auth_method?: string   // 认证方式: none/pap/chap
}

// Ping 结果
export interface PingResult {
  success: boolean       // 是否成功
  latency_ms?: number    // 延迟（毫秒）
  target: string         // 目标地址
  error?: string         // 错误信息
}

// 联网检测响应
export interface ConnectivityCheckResponse {
  ipv4: PingResult       // IPv4 连通性
  ipv6: PingResult       // IPv6 连通性
}

// ============ 通话记录类型 ============

// 通话记录
export interface CallRecord {
  id: number
  direction: string       // "incoming" / "outgoing" / "missed"
  phone_number: string
  duration: number        // 通话时长（秒）
  start_time: string      // 开始时间 ISO 8601
  end_time?: string       // 结束时间 ISO 8601
  answered: boolean       // 是否接通
}

// 通话统计
export interface CallStats {
  total: number
  incoming: number
  outgoing: number
  missed: number
  total_duration: number  // 总通话时长（秒）
}

// 通话记录响应
export interface CallHistoryResponse {
  records: CallRecord[]
  stats: CallStats
}

// ============ init.sh 管理类型 ============

export interface InitScriptResponse {
  script: string
  init_path: string
  loader_path: string
  loader_hooked: boolean
}

export interface SetInitScriptRequest {
  script: string
}

// ============ Webhook 配置类型 ============

// Webhook 配置
export interface WebhookConfig {
  enabled: boolean
  url: string
  forward_sms: boolean
  forward_calls: boolean
  headers: Record<string, string>
  secret: string
  sms_template: string    // 短信 payload 模板
  call_template: string   // 通话 payload 模板
}

// 默认短信模板 (飞书机器人格式)
export const DEFAULT_SMS_TEMPLATE = `{
  "msg_type": "text",
  "content": {
    "text": "📱 短信通知\\n发送方: {{phone_number}}\\n内容: {{content}}\\n时间: {{timestamp}}"
  }
}`

// 默认通话模板 (飞书机器人格式)
export const DEFAULT_CALL_TEMPLATE = `{
  "msg_type": "text",
  "content": {
    "text": "📞 来电通知\\n号码: {{phone_number}}\\n类型: {{direction}}\\n时间: {{start_time}}\\n时长: {{duration}}秒\\n已接听: {{answered}}"
  }
}`

// Webhook 测试响应
export interface WebhookTestResponse {
  success: boolean
  message: string
}

// ============ 短信推送配置类型 ============

export type SmsPushProvider = 'pushplus' | 'serverchan' | 'pushdeer' | 'bark' | 'ntfy'

export interface SmsPushConfig {
  enabled: boolean
  provider: SmsPushProvider
  credential: string
  server_url: string
  topic: string
  title_template: string
  body_template: string
}

export const DEFAULT_SMS_PUSH_TITLE_TEMPLATE = '短信通知 · {{phone_number}}'

export const DEFAULT_SMS_PUSH_BODY_TEMPLATE = `时间: {{timestamp}}
号码: {{phone_number}}
状态: {{status}}

{{content}}`

// ========== OTA 更新类型 ==========

// OTA 元数据
export interface RefreshConfigResponse {
  interval_ms: number
  watchdog_active_interval_ms: number
  watchdog_idle_interval_ms: number
  heartbeat_timeout_ms: number
  frontend_connected: boolean
}

export interface OtaMeta {
  version: string
  commit: string
  build_time: string
  binary_md5: string
  frontend_md5: string
  arch: string
  min_version?: string
}

// OTA 验证结果
export interface OtaValidation {
  valid: boolean
  is_newer: boolean
  binary_md5_match: boolean
  frontend_md5_match: boolean
  arch_match: boolean
  error?: string
}

// OTA 状态响应
export interface OtaStatusResponse {
  current_version: string
  current_commit: string
  pending_update: boolean
  pending_meta?: OtaMeta
}

// OTA 上传响应
export interface OtaUploadResponse {
  meta: OtaMeta
  validation: OtaValidation
}

// OTA 应用请求
export interface OtaApplyRequest {
  restart_now: boolean
}

