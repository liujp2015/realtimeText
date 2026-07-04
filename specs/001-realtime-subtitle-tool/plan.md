# Implementation Plan: Windows 实时字幕工具

**Branch**: `001-realtime-subtitle-tool` | **Date**: 2026-06-30 | **Last Updated**: 2026-06-30（重构为 SSE + 本地 VAD）| **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-realtime-subtitle-tool/spec.md`

> **架构变更说明（2026-06-30）**：原方案采用 WebSocket 双向流式接口（`stepaudio-2.5-asr-stream`，1.2 元/小时），因成本过高重构为 SSE 一次性接口（`stepaudio-2.5-asr`，0.15 元/小时）+ 本地 silero-vad 切句。成本降低 8 倍，代价是延迟从近实时变为「句子结束 + SSE 往返」（典型 1-3s）。旧的 WebSocket 协议契约保留在 `contracts/ws-protocol.md` 但已废弃，现行协议见 `contracts/sse-protocol.md`。

## Summary

构建一款仅面向 Windows 的桌面级实时字幕工具，通过 WASAPI Loopback 捕获系统播放器输出音频，经 DSP 管道降采样为 16kHz/单声道/pcm_s16le，由本地 silero-vad（ONNX 推理）按语音活动检测切句（静音 ≥800ms 或单句 ≥10s 强制提交），每句一次性 POST 至阶跃星辰 StepAudio 2.5 ASR 的 SSE 接口流式接收识别结果，通过 Tauri v2 IPC 推送至悬浮透明 Webview 渲染滚动字幕，稳态结果持久化到本地 SQLite，支持会话回看与关键词检索。技术栈以 Tauri v2（Rust 后端 + Web 前端）为核心，cpal 做音频抽象，rubato 做重采样，ort 做 ONNX 推理，reqwest 做 SSE 请求，sqlx 做异步持久化，tauri-plugin-log 做日志轮转。

## Technical Context

**Language/Version**: Rust 1.80+（后端，ort 2.x MSRV 要求）+ TypeScript（前端，Vue 3）

**Primary Dependencies**:
- `tauri` v2 — 桌面框架与 IPC
- `cpal` — 跨平台音频抽象（Windows 下基于 WASAPI Loopback，`build_input_stream` 在 output device 上做回环捕获）
- `rubato` — 线性相位重采样
- `rtrb` — 无锁环形缓冲区
- `ort` 2.0.0-rc.12 — ONNX Runtime 推理（silero-vad 模型）
- `ndarray` 0.17 — ort 张量数据结构
- `include_dir` — 编译期嵌入 onnx 模型文件
- `reqwest` 0.12（rustls-tls）— SSE HTTP 请求与流式响应解析
- `tokio` — 异步运行时
- `sqlx`（SQLite driver）— 异步数据库访问，编译期 SQL 校验
- `tauri-plugin-log` — 日志收集与轮转
- `serde` / `serde_json` — 序列化
- `base64` — 音频帧编码
- 前端：`@tauri-apps/api` v2（event / window），Vue 3 + Vite + Pinia

**Storage**: SQLite（单文件，位于 `%APPDATA%\<bundleId>\`），通过 sqlx 编译期迁移

**Testing**: `cargo test`（Rust 单元/集成），前端 Vitest，端到端手测脚本见 `quickstart.md`

**Target Platform**: Windows 10 / Windows 11（x64），仅 Windows 构建

**Project Type**: desktop-app（Tauri v2）

**Performance Goals**:
- 端到端延迟（不含模型固有处理）：句尾静音 800ms + SSE 往返，典型 1-3s
- 音频回调线程无阻塞，零 overrun/underrun
- VAD 推理延迟 ≤ 5ms/帧（512 samples @16k）
- 30 分钟连续识别内存增长 ≤ 初始 30%
- 关键词检索 95% 查询 ≤ 1s

**Constraints**:
- 仅 Windows 10/11 x64
- 仅捕获播放器回环音频，不采集麦克风
- 不上传任何数据至除阶跃星辰识别服务外的第三方
- 8 小时日志总量 ≤ 50 MB
- 长时间运行无内存泄漏
- 副语言信息（情绪/语速）SSE 接口不返回，已丢弃（前端字段保留置空）

**Scale/Scope**: 单用户单设备桌面应用；会话级数据规模（典型单会话数百条字幕）；历史检索跨数百会话

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

项目 `.specify/memory/constitution.md` 当前为模板占位状态（`[PRINCIPLE_1_NAME]` 等未填写），无实际治理原则可校验。Constitution Check 视为 PASS（无约束可违反）。

**建议（非阻塞）**：在本特性交付后，团队应将 constitution.md 填写为正式版本，明确如下原则以约束后续特性：
- 隐私优先：用户数据本地化，禁止上传第三方
- 实时线程禁阻塞：音频回调线程禁止内存分配/锁/I/O
- 离线降级：网络中断时本地缓冲不丢音频
- 日志可控：必须启用轮转

Post-Phase-1 复检：Phase 1 设计（data-model / contracts / quickstart）未引入与上述建议冲突的元素，仍 PASS。

## Project Structure

### Documentation (this feature)

```text
specs/001-realtime-subtitle-tool/
├── plan.md              # 本文件
├── research.md          # Phase 0 研究结论
├── data-model.md        # Phase 1 数据模型
├── quickstart.md        # Phase 1 验证指南
├── contracts/           # Phase 1 接口契约
│   ├── ipc-events.md    # Tauri IPC 事件契约
│   ├── ws-protocol.md   # 阶跃星辰 WebSocket 协议契约
│   └── commands.md      # Tauri Command 契约
└── tasks.md             # Phase 2 输出（/speckit-tasks，本命令不创建）
```

### Source Code (repository root)

```text
src-tauri/                       # Rust 后端
├── Cargo.toml
├── tauri.conf.json              # 窗口与权限配置
├── capabilities/
│   └── default.json             # Tauri 2 权限声明（event:listen 等）
├── assets/
│   └── silero_vad.onnx          # silero-vad v5 模型（include_dir 嵌入）
├── migrations/
│   └── 0001_init.sql            # sqlx 迁移
└── src/
    ├── main.rs                  # Tauri Builder 入口
    ├── lib.rs
    ├── state.rs                 # AppState（DB pool、配置、运行时句柄）
    ├── audio/
    │   ├── mod.rs
    │   ├── capture.rs           # WASAPI Loopback 捕获（cpal build_input_stream）
    │   ├── ring.rs              # rtrb 无锁环形缓冲封装
    │   └── dsp.rs               # 下混/重采样/量化（next_f32_frame / next_frame）
    ├── vad/                     # 本地语音活动检测
    │   ├── mod.rs
    │   ├── silero.rs            # silero-vad ONNX 推理（含 64-sample context）
    │   └── state.rs             # 状态机：10s force / 800ms silence 切句
    ├── asr/
    │   ├── mod.rs
    │   ├── client.rs            # reqwest SSE POST + 流式解析
    │   ├── protocol.rs          # SSE 请求/响应结构与序列化
    │   └── pipeline.rs          # 音频→VAD→utterance channel→drainer 串行提交
    ├── db/
    │   ├── mod.rs
    │   ├── pool.rs              # sqlx Pool 初始化与迁移
    │   └── repository.rs        # 会话/字幕/检索 CRUD
    ├── commands/
    │   ├── mod.rs               # #[tauri::command] 暴露给前端
    │   ├── session.rs           # start/stop/list/get/delete
    │   ├── search.rs            # 关键词检索
    │   └── config.rs            # 样式/位置/API Key 读写
    ├── events.rs                # emit subtitle-update / asr-status / session-meta
    └── logging.rs               # tauri-plugin-log 配置 + 前端 console 拦截

src/                             # 前端（Vue 3 + Vite）
├── main.ts
├── App.vue
├── windows/
│   ├── subtitle/                # 悬浮字幕窗
│   │   ├── App.vue
│   │   └── style.css
│   └── dashboard/               # 设置/历史面板
│       ├── App.vue
│       └── views/
│           ├── Settings.vue
│           └── History.vue
├── composables/
│   ├── useSubtitle.ts           # listen subtitle-update
│   ├── useAsrStatus.ts
│   └── useAppearance.ts
├── stores/                      # pinia
│   └── settings.ts
└── lib/
    └── tauri.ts                 # 封装 invoke / listen

tests/                           # Rust 集成测试（cargo test）
├── audio/
│   ├── dsp_test.rs              # 重采样/量化正确性
│   └── ring_test.rs             # 环形缓冲压力测试
├── vad/
│   └── state_test.rs            # VAD 状态机时序测试
├── asr/
│   └── protocol_test.rs         # SSE 事件序列化
└── db/
    └── repository_test.rs       # SQLite CRUD
```

**Structure Decision**: 采用标准 Tauri v2 双目录结构（`src-tauri/` Rust 后端 + `src/` 前端）。Rust 后端按职责垂直切分为 `audio/ vad/ asr/ db/ commands/ events/ logging/` 七个模块，避免循环依赖；前端按窗口维度切分（`subtitle` 悬浮窗 vs `dashboard` 设置/历史面板），共享 composables 与 stores。多窗口架构天然契合 Tauri 事件总线。VAD 模块独立于 ASR 模块，便于未来替换切句策略（如能量阈值、WebRTC VAD）。

## Complexity Tracking

> 无 Constitution Check 违规需要 justify。本节为空。
