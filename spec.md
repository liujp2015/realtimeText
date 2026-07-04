# 实时字幕工具产品规格说明书（spec.md）

> 基于《基于 Tauri v2 与阶跃星辰端到端大模型的全局实时语音捕获与字幕渲染系统架构研究》提炼
> 范围限定：仅 Windows 平台；音频源为系统播放器输出（WASAPI Loopback）

## 1. 项目概述

### 1.1 目标
构建一款 Windows 桌面级实时语音字幕工具，通过 WASAPI Loopback 捕获系统播放器（视频播放器、浏览器、会议软件等）的音频输出，经阶跃星辰 StepAudio 2.5 Realtime 端到端语音大模型流式识别，以悬浮透明窗口形式在屏幕上层渲染实时字幕，并将历史字幕与副语言元数据持久化到本地 SQLite 数据库，供后续检索与分析。

### 1.2 参考产品
- Wispr Flow（沉浸式悬浮字幕交互形态）

### 1.3 技术栈基线
- 桌面框架：Tauri v2（Rust 后端 + 系统 Webview 前端），仅构建 Windows 目标
- 音频捕获：cpal（基于 WASAPI Loopback）
- DSP：rubato（重采样）+ 自实现混音/量化
- 网络通信：tokio-tungstenite（WebSocket 全双工）
- 语音模型：阶跃星辰 StepAudio 2.5 Realtime（`wss://api.stepfun.com/v1/realtime/asr/stream`）
- 持久化：SQLite + sqlx（异步、编译期校验）
- 日志：tauri-plugin-log
- 前端：Vue 3 Composition API 或 React Hooks + @tauri-apps/api/event

## 2. 功能性需求

### 2.1 音频捕获
| 编号 | 需求 | 优先级 |
| --- | --- | --- |
| F-AUDIO-01 | 通过 WASAPI Loopback 捕获系统默认输出设备（扬声器/耳机）的播放器音频 | P0 |
| F-AUDIO-02 | 支持用户在 UI 中手动选择特定输出设备（多扬声器/蓝牙耳机场景） | P1 |
| F-AUDIO-03 | 监听默认输出设备变更（如蓝牙连接/断开），自动切换捕获设备并重建流 | P1 |
| F-AUDIO-04 | 启动/停止捕获的开关控制，停止时优雅关闭流与 WebSocket | P0 |

> 说明：本工具不采集麦克风输入，仅处理播放器回环音频。

### 2.2 实时识别
| 编号 | 需求 | 优先级 |
| --- | --- | --- |
| F-ASR-01 | 通过 WebSocket 与阶跃星辰建立持久全双工连接，Header 鉴权 `Authorization: Bearer $STEPFUN_API_KEY` | P0 |
| F-ASR-02 | 连接建立后主动发送 `session.update` 配置信令，参数见 §3.2 | P0 |
| F-ASR-03 | 持续以 `input_audio_buffer.append` 推送 40ms 帧（1280 字节 PCM） | P0 |
| F-ASR-04 | 接收并区分中间非稳态结果（动态草稿）与稳态结果（VAD 判定句尾） | P0 |
| F-ASR-05 | WebSocket 断连时启用指数退避重连，期间内存缓存音频待重连补发 | P0 |
| F-ASR-06 | 副语言特征（情绪/语速/笑声/叹息）随稳态结果一并接收 | P1 |

### 2.3 字幕渲染
| 编号 | 需求 | 优先级 |
| --- | --- | --- |
| F-UI-01 | 提供无边框、透明、置顶的悬浮字幕窗口（`transparent`/`decorations:false`/`alwaysOnTop`） | P0 |
| F-UI-02 | 支持鼠标事件穿透（Ignore cursor events），不阻塞底层应用交互 | P0 |
| F-UI-03 | 实时滚动展示当前草稿句，句尾后归档至历史列表 | P0 |
| F-UI-04 | 字幕样式（字体、字号、颜色、底板透明度）可配置 | P1 |
| F-UI-05 | 支持字幕窗口位置拖拽与位置记忆 | P1 |

### 2.4 数据持久化与检索
| 编号 | 需求 | 优先级 |
| --- | --- | --- |
| F-DB-01 | 稳态识别结果与副语言元数据写入本地 SQLite | P0 |
| F-DB-02 | 按 `session_guid` 维度组织会话，支持会话级时间轴重建 | P0 |
| F-DB-03 | 暴露 `fetch_history(session_id)` / `search_keywords(keyword)` 等 Tauri Command 供前端调用 | P1 |
| F-DB-04 | 数据库文件位于系统应用数据沙盒目录（`app_data_dir()`，Windows 为 `%APPDATA%\<bundleId>`） | P0 |

### 2.5 监控与可观测性
| 编号 | 需求 | 优先级 |
| --- | --- | --- |
| F-OBS-01 | 集成 tauri-plugin-log，统一写入系统日志目录 | P0 |
| F-OBS-02 | 启用日志轮转，单文件上限 50,000 字节 | P0 |
| F-OBS-03 | 拦截前端 `console.log`/`console.error` 同步写入后端日志 | P1 |
| F-OBS-04 | 统一本地时区时间戳（`TimezoneStrategy::UseLocal`） | P1 |

## 3. 技术规格

### 3.1 DSP 管道
捕获端原始格式（Windows WASAPI 默认输出典型配置）：48000 Hz / 立体声 / f32 交错 → 目标格式：16000 Hz / 单声道 / pcm_s16le。

处理顺序：
1. 从无锁环形缓冲区（rtrb）拉取交错 f32 样本
2. 立体声下混：`mono = (L + R) / 2`
3. 抗混叠低通滤波 + 下采样 48k → 16k（rubato，线性相位）
4. 硬限幅 [-1.0, 1.0]
5. 量化：`i16 = round(f32 * 32767.0)`
6. 按 40ms 帧累积（16000 × 0.04 × 2B = 1280 字节）
7. Base64 编码后通过 WebSocket 发送

约束：
- 音频回调运行在 WASAPI 实时优先级线程，**禁止**内存分配、互斥锁、磁盘 I/O、网络 I/O
- 回调唯一职责：将原始字节压入无锁环形缓冲区后立即返回
- 重活全部在独立工作线程完成

### 3.2 WebSocket 会话配置信令
`session.update` 必填字段：

| JSON 路径 | 值 |
| --- | --- |
| `type` | `"session.update"` |
| `session.audio.input.format.type` | `"pcm"` |
| `session.audio.input.format.codec` | `"pcm_s16le"` |
| `session.audio.input.format.rate` | `16000` |
| `session.audio.input.format.bits` | `16` |
| `session.audio.input.format.channel` | `1` |

### 3.3 IPC 事件契约
| 事件名 | 方向 | Payload | 说明 |
| --- | --- | --- | --- |
| `subtitle-update` | Rust → 前端 | `{ state: "partial" \| "final", text, start_ts, end_ts, paralinguistic? }` | 单句字幕更新 |
| `session-meta` | Rust → 前端 | `{ session_guid, started_at }` | 会话启动通知 |
| `asr-status` | Rust → 前端 | `{ connected: bool, retry_count, last_error? }` | 连接状态 |
| `fetch-history` | 前端 → Rust | `{ session_id? }` | 查询历史 |
| `search-keywords` | 前端 → Rust | `{ keyword }` | 关键词检索 |

前端监听器必须在组件卸载时 `await` unlisten Promise 并调用反注册函数，避免事件总线泄漏。

### 3.4 数据库表结构
```sql
CREATE TABLE transcriptions (
    id                     INTEGER PRIMARY KEY AUTOINCREMENT,
    session_guid           TEXT NOT NULL,
    transcription_text     TEXT NOT NULL,
    start_timestamp        BIGINT NOT NULL,
    end_timestamp          BIGINT NOT NULL,
    paralinguistic_metadata TEXT,  -- JSON 字符串
    created_at             BIGINT NOT NULL DEFAULT (strftime('%s','now'))
);
CREATE INDEX idx_session_guid ON transcriptions(session_guid);
CREATE INDEX idx_created_at ON transcriptions(created_at);
```

迁移：通过 `sqlx::migrate!("./migrations")` 在首次启动时执行。

### 3.5 窗口配置（tauri.conf.json 关键项）
```json
{
  "transparent": true,
  "decorations": false,
  "alwaysOnTop": true,
  "skipTaskbar": true,
  "resizable": true
}
```
运行时通过 Tauri Window API 动态切换鼠标穿透。

## 4. 非功能性需求

| 维度 | 指标 |
| --- | --- |
| 端到端延迟 | 从音频捕获到字幕渲染 ≤ 1.5s（排除模型固有延迟） |
| 模型中位延迟 | StepAudio 2.5 Realtime 基准 9.5s（参考） |
| CPU 占用 | 空闲 < 2%，识别中 < 15%（单核归一） |
| 内存占用 | 常驻 < 200MB |
| 音频缓冲 | 无锁环形缓冲区，无 overrun/underrun |
| 兼容性 | Windows 10 / Windows 11（x64） |
| 隐私 | 全部数据本地存储，不上传除阶跃星辰 API 之外任何第三方 |
| 鲁棒性 | WebSocket 断连自动重连，音频不丢失 |

## 5. 约束与假设
- 用户需自备阶跃星辰 API Key（`STEPFUN_API_KEY` 环境变量或应用内配置）
- 仅支持 Windows 10/11，不提供 macOS / Linux 构建
- 音频源限定为系统播放器输出（WASAPI Loopback），不采集麦克风
- 不支持 DRM 保护音频的捕获

## 6. 里程碑
| 阶段 | 交付物 |
| --- | --- |
| M1 | WASAPI Loopback 捕获播放器音频 + DSP 管道 + 本地落盘验证 |
| M2 | WebSocket 接入阶跃星辰，控制台打印识别结果 |
| M3 | Tauri 透明悬浮窗 + IPC 事件 + 字幕滚动 |
| M4 | SQLite 持久化 + 历史/检索 Command |
| M5 | 日志插件、重连容错、Windows 打包发布（MSI/NSIS） |
