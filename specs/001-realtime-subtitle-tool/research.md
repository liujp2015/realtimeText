# Phase 0 Research: Windows 实时字幕工具

**Date**: 2026-06-30
**Status**: Complete
**Source**: 架构文档《基于 Tauri v2 与阶跃星辰的实时字幕工具开发架构》+ spec.md

## 研究任务清单

Technical Context 中无 NEEDS CLARIFICATION 项（架构文档已为所有关键技术决策提供依据）。本研究聚焦于把架构文档中的决策显式落地为可执行结论，并补充文档未覆盖的若干工程细节。

---

## R1: Windows 全局音频捕获方案

**Decision**: 使用 `cpal` crate，在 Windows 上通过 WASAPI Loopback 捕获系统默认输出设备的播放器音频。

**Rationale**:
- WASAPI 原生支持 `AUDCLNT_STREAMFLAGS_LOOPBACK`，可镜像输出流为输入流，无需虚拟驱动
- `cpal` 在 Windows 下已封装 WASAPI Loopback（架构文档引用 #1、#2、#15）
- 不采集麦克风：仅取默认输出设备，不取输入设备

**Alternatives considered**:
- 直接调用 `wasapi` crate：更底层但需自行管理 COM 初始化与设备枚举，收益不抵成本
- 使用 PyAudioWPatch（Python）：与 Tauri/Rust 后端技术栈不兼容
- 虚拟声卡驱动（如 VB-Cable）：增加用户分发摩擦，违背「免驱动」目标

**关键实现细节**:
- 取 `default_host()` → `default_output_device()`（非 input）
- 配置采样率 48000Hz、双声道、f32 交错
- 回调运行在 WASAPI 实时优先级线程，**禁止**内存分配、互斥锁、磁盘/网络 I/O
- 回调唯一职责：把原始字节压入 `rtrb` 无锁环形缓冲后立即返回

---

## R2: DSP 管道与重采样

**Decision**: 48000Hz/立体声/f32 → 16000Hz/单声道/pcm_s16le，使用 `rubato` 做线性相位重采样。

**Rationale**:
- 阶跃星辰 API 强制要求 16kHz/单声道/pcm_s16le（架构文档 #16）
- 直接降采样会引发高频混叠，必须先抗混叠低通滤波
- `rubato` 提供线性相位 FIR 重采样，相位线性、质量可靠

**处理链**:
1. 从 rtrb 拉取交错 f32 样本
2. 立体声下混：`mono = (L + R) / 2`
3. 抗混叠低通滤波 + 48k → 16k（rubato）
4. 硬限幅 [-1.0, 1.0]
5. 量化：`i16 = round(f32 * 32767.0)`
6. 累积 40ms 帧 = 16000 × 0.04 × 2B = 1280 字节
7. Base64 编码

**Alternatives considered**:
- 简单抽取（每三个样本取一个）：会引入混叠，舍弃
- FFT 卷积自实现：复杂度高，rubato 已封装

---

## R3: WebSocket 协议与阶跃星辰集成

**Decision**: 使用 `tokio-tungstenite` 建立到 `wss://api.stepfun.com/v1/realtime/asr/stream` 的全双工连接，Header 鉴权 `Authorization: Bearer $STEPFUN_API_KEY`，连接建立后立即发送 `session.update` 配置信令。

**Rationale**:
- 端到端模型延迟（中位 9.5s）远优于级联管线（Azure 73.7s），副语言理解 82.18 分（架构文档 #20）
- WebSocket 全双工适合持续音频流推送
- 服务端内置 VAD，客户端无需本地 VAD

**session.update 必填字段**:

| JSON 路径 | 值 |
| --- | --- |
| `type` | `"session.update"` |
| `session.audio.input.format.type` | `"pcm"` |
| `session.audio.input.format.codec` | `"pcm_s16le"` |
| `session.audio.input.format.rate` | `16000` |
| `session.audio.input.format.bits` | `16` |
| `session.audio.input.format.channel` | `1` |

**两类下发事件**:
- 中间非稳态结果（partial）：动态变化的草稿文本，用于前端滚动
- 稳态结果（final）：VAD 检测到句尾静音时下发，含起止时间戳与最终文本

**Alternatives considered**:
- HTTP 短链接分块上传：延迟高、无全双工，舍弃
- 本地 VAD + 分段识别：与端到端模型设计相悖，舍弃

---

## R4: 断连重连策略

**Decision**: 指数退避重连，断连期间音频在内存环形缓冲中暂存（容量上限 30s），重连成功后批量补发；超过 30s 的音频若仍未重连则丢弃并向用户提示。

**Rationale**:
- WebSocket 长连接对网络波动敏感
- 无限重试 + 无限缓存会导致内存失控
- 30s 缓冲可覆盖典型短暂网络抖动，与 SC-005（5 秒内补齐）兼容

**重连间隔序列**: 1s, 2s, 4s, 8s, 16s, 30s（封顶 30s）

**Alternatives considered**:
- 固定 1s 重连：网络持续故障时刷屏日志
- 不补发、丢弃断连期间音频：违反 SC-005
- 永久缓存到磁盘：增加 I/O 复杂度，收益不抵

---

## R5: Tauri IPC 事件契约

**Decision**: 使用 Tauri v2 `Emitter` 接口，从 Rust 向前端广播以下事件：

| 事件名 | Payload |
| --- | --- |
| `subtitle-update` | `{ state: "partial"\|"final", text, start_ts, end_ts?, paralinguistic? }` |
| `session-meta` | `{ session_guid, started_at }` |
| `asr-status` | `{ connected: bool, retry_count, last_error? }` |

前端通过 `@tauri-apps/api/event` 的 `listen` 订阅，组件卸载时 `await` unlisten Promise 并调用反注册函数，避免事件总线泄漏。

**Alternatives considered**:
- 轮询 Rust command：延迟高、CPU 浪费
- 共享内存：跨语言复杂度高，收益不抵

---

## R6: 悬浮透明窗口与事件穿透

**Decision**: tauri.conf.json 配置 `transparent: true`、`decorations: false`、`alwaysOnTop: true`、`skipTaskbar: true`；运行时通过 Tauri Window API 动态切换鼠标穿透（`set_ignore_cursor_events(true)`）。

**Rationale**:
- 沉浸式字幕需要无边框透明置顶
- 字幕窗不应阻塞底层播放器的鼠标交互（FR-003、SC-003）
- Tauri v2 在 Windows 上支持 `set_ignore_cursor_events`

**交互细节**:
- 默认开启鼠标穿透，让点击直达底层播放器
- 用户拖动字幕窗时临时关闭穿透（如通过快捷键或悬浮「拖拽手柄」区域），松开后恢复穿透
- 字幕窗不显示在任务栏，避免占用任务栏位置

**Alternatives considered**:
- 始终关闭穿透：违反 FR-003
- 仅在文字区域穿透、底板区域不穿透：实现复杂且行为不直观

---

## R7: SQLite 持久化方案

**Decision**: 使用 `sqlx` + SQLite，数据库文件位于 `app_handle.path().app_data_dir()`（Windows 下为 `%APPDATA%\<bundleId>\`），通过 `sqlx::migrate!("./migrations")` 执行编译期迁移。

**Rationale**:
- SQLite 零运维、单文件、ACID，是桌面应用持久化行业标准
- `sqlx` 异步、不阻塞 Tokio、支持编译期 SQL 校验，优于同步的 `rusqlite`
- Tauri 提供安全的路径解析 API，避免硬编码

**表结构**: 见 `data-model.md`

**连接池管理**: `sqlx::SqlitePool` 注入 Tauri `State`，跨 command 共享。

**Alternatives considered**:
- JSON 文件：高频写入下损坏风险高、无索引、查询慢，舍弃
- `rusqlite` + `Mutex<Connection>`：阻塞 Tokio 风险，舍弃
- tauri-plugin-sql：可行但抽象层薄、迁移支持弱，选择 sqlx 直用

---

## R8: 日志与可观测性

**Decision**: 集成 `tauri-plugin-log`，配置 `TargetKind::LogDir`、`max_file_size(50_000)`、`TimezoneStrategy::UseLocal`，并通过 `forwardConsole` 拦截前端 `console.log/error` 同步写入后端日志。

**Rationale**:
- 统一前后端日志到同一物理文件，便于全链路 RCA
- 50KB 自动轮转满足 SC-007（8 小时 ≤ 50MB）
- 本地时区保证时间戳一致

**日志级别约定**:
- `error`: 连接断开、API Key 无效、数据库错误
- `warn`: 重连尝试、设备切换、缓冲丢弃
- `info`: 会话开始/结束、连接建立
- `debug`: 帧计数、识别结果摘要（生产可关）
- `trace`: 仅开发期

**Alternatives considered**:
- `tracing` + `tracing-subscriber`：更现代但与 Tauri 前端日志统一成本高
- 自实现日志：重复造轮子

---

## R9: 默认输出设备变更监听

**Decision**: 定期（每 2 秒）轮询 `cpal::default_host().default_output_device()`，与当前捕获设备对比；若变更则停止旧流、启动新流，并通过 `asr-status` 事件提示用户。中断恢复时间目标 ≤ 3 秒（SC-008）。

**Rationale**:
- `cpal` 未提供设备变更回调（截至当前版本），轮询是最简方案
- 2 秒轮询间隔在 3 秒 SLA 内留出 1 秒重建流余量
- 蓝牙耳机插拔、USB 声卡热插拔是高频场景

**Alternatives considered**:
- Windows 多媒体事件订阅（`IMMNotificationClient`）：更实时但 COM 代码复杂，收益不抵
- 让用户手动重启：违反 SC-008

---

## R10: API Key 与配置存储

**Decision**: API Key 与字幕样式配置统一存于 SQLite 的 `app_config` 表（key-value），通过 Tauri command 暴露读写。API Key 在数据库中以明文存储（单用户本地设备，OS 文件权限已提供边界保护）。

**Rationale**:
- 单用户单设备场景下，明文存储 + OS 文件权限足够
- 加密存储需要管理密钥（DPAPI 或自管密钥），增加复杂度，收益有限
- 集中在 SQLite 便于备份与迁移

**配置项**:
- `api_key`: 阶跃星辰密钥
- `appearance.font_family`、`appearance.font_size`、`appearance.text_color`、`appearance.bg_opacity`
- `window.x`、`window.y`、`window.width`、`window.height`

**Alternatives considered**:
- 环境变量：分发后用户配置困难
- Windows DPAPI 加密：v1 范围外，列为后续改进

---

## 研究结论汇总

| ID | 主题 | 决策 |
| --- | --- | --- |
| R1 | 音频捕获 | cpal + WASAPI Loopback |
| R2 | DSP 管道 | rubato 线性相位重采样，40ms/1280B 帧 |
| R3 | WebSocket 协议 | tokio-tungstenite + session.update |
| R4 | 重连策略 | 指数退避，30s 内存缓冲 |
| R5 | IPC 事件 | subtitle-update / session-meta / asr-status |
| R6 | 悬浮窗 | transparent + alwaysOnTop + 动态鼠标穿透 |
| R7 | 持久化 | sqlx + SQLite + 编译期迁移 |
| R8 | 日志 | tauri-plugin-log + 50KB 轮转 + 前端 console 拦截 |
| R9 | 设备变更 | 2s 轮询默认输出设备 |
| R10 | 配置存储 | SQLite app_config 表，明文 API Key |

所有 NEEDS CLARIFICATION 已解析，可进入 Phase 1。
