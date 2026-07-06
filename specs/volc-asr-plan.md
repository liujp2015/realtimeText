# 火山引擎实时 ASR 集成方案

## 目标
在现有阶跃星辰 SSE ASR 之上，**新增**火山引擎（字节跳动）实时语音转写作为可选 ASR provider。设置页可选择 ASR 服务并填写对应 API Key / 模型。保留现有阶跃实现，两者可切换。

## 现有架构（集成点）
- `asr/client.rs::submit_utterance(api_key, pcm_s16le, start, end) -> Receiver<AsrEvent>` —— 阶跃 SSE 实现
- `AsrEvent` enum（`Partial`/`Final`/`Error`）—— client 与 pipeline 的统一契约
- `pipeline.rs::run_drainer` —— 每个 VAD 切出的 utterance → 量化 PCM → 调 `submit_utterance` → 读 AsrEvent 流 → emit subtitle + 写 DB
- `session_start` —— 从 `config.api_key` 读 key → 传给 `pipeline::spawn`
- 配置：SQLite key-value（`get/set_config_value`）+ `AppConfig` 内存镜像 + `config_set` 命令 mirror

## 方案

### 1. 配置层（`config.rs` + `commands/config.rs` + `lib.rs`）
`AppConfig` 新增字段：
- `provider: AsrProvider`（enum `Stepfun`|`Volc`，serde lowercase，默认 `Stepfun`）
- `volc_api_key: String`
- `volc_resource_id: String`（默认 `volc.seedasr.sauc.duration`）
- `volc_model: String`（默认 `bigmodel`）
- `volc_url: String`（默认 `wss://openspeech.bytedance.com/api/v3/plan/sauc/bigmodel_async`）

`config_set` 加对应 mirror 分支；`load_config` 加载新字段。

### 2. Provider 抽象（新建 `asr/provider.rs`）
- `AsrConfig` enum：`Stepfun { api_key }` | `Volc { api_key, resource_id, url, model }`
- 统一入口 `submit_utterance(cfg, pcm, start, end) -> Result<Receiver<AsrEvent>>`，内部 match 分发
- `pipeline.rs::spawn`：参数 `api_key: String` → `asr: AsrConfig`；`run_drainer` 改调 `provider::submit_utterance`
- `session_start`：读 config 构造 `AsrConfig` 传给 pipeline

### 3. 火山引擎客户端（新建 `asr/volc.rs`）
`submit_utterance_volc(api_key, resource_id, url, model, pcm_s16le, start, end) -> Result<Receiver<AsrEvent>>`，移植 Python 的二进制 WS 协议：
- 鉴权 headers：`X-Api-Key`/`X-Api-Resource-Id`/`X-Api-Request-Id`/`X-Api-Connect-Id`/`X-Api-Sequence: -1`
- 帧格式：4 字节 header（version|hdr_size, msg_type|flags, serialization|compression, reserved）+ seq(i32 BE) + payload_size(u32 BE) + gzip(payload)
- 流程：full client request（JSON 配置 payload）→ 分段 audio-only（每 ~200ms，gzip）→ last 包（`NEG_WITH_SEQUENCE`，seq 取负）
- 响应解析：`header_size = msg[0]&0x0f`、`message_type = msg[1]>>4`、flags、serialization、compression → 解压 → JSON
- 并发：tokio task 发音频 + 收响应循环；`is_last_package` 或 `code!=0` 结束
- →`AsrEvent` 映射：`result.text` → `Partial`；`is_last_package` → `Final`；`SERVER_ERROR_RESPONSE` → `Error`
- 加单测验证 header 编解码（字节序/flags）

### 4. 依赖（`Cargo.toml`）
- `tokio-tungstenite = { version = "0.21", features = ["rustls-tls-webpki-roots"] }`（WS + rustls，与现有 reqwest rustls 一致）
- `flate2 = "1"`（gzip）
纯 Rust，跨平台含 Android target 可编译。

### 5. 前端
- `stores/settings.ts`：加 `provider`/`volcApiKey`/`volcResourceId`/`volcModel`/`volcUrl` ref + load/save
- `Settings.vue`：
  - "ASR 服务" 下拉：阶跃星辰 / 火山引擎
  - 阶跃分支：现有 API Key 输入
  - 火山分支：API Key + Resource ID（预填默认）+ 模型（预填 `bigmodel`）+ URL（预填默认，折叠为"高级"）
  - 各自保存按钮 + 已保存提示
- `lib/tauri.ts`：`configGet/Set` 已通用，无需改

### 6. 文件清单
**新增**
- `src-tauri/src/asr/volc.rs`
- `src-tauri/src/asr/provider.rs`

**修改**
- `src-tauri/Cargo.toml`（依赖）
- `src-tauri/src/config.rs`（`AppConfig` 字段 + `AsrProvider` enum + 默认值）
- `src-tauri/src/commands/config.rs`（`config_set` mirror）
- `src-tauri/src/lib.rs`（`load_config` 加载新字段）
- `src-tauri/src/asr/mod.rs`（声明 `volc`/`provider`）
- `src-tauri/src/asr/pipeline.rs`（`spawn` 签名 + `run_drainer` 用 `AsrConfig`）
- `src-tauri/src/commands/session.rs`（构造 `AsrConfig`，按 provider 校验对应 key）
- `src/stores/settings.ts`（新字段 + load/save）
- `src/windows/dashboard/views/Settings.vue`（UI）

## 风险 / 注意
- 火山响应 JSON 内部结构（`result.text` / `utterances[].definite`）需实联调确认 → 防御性解析 + `log::info` 打印原始 payload
- 二进制协议严格按 Python 移植，加单测保证 header 编解码正确
- 流式模式：每个 utterance 独立建一次 WS（与阶跃每句一次 SSE 一致），utterance 内仍收多 partial
- Android 编译：tungstenite+rustls+flate2 纯 Rust 应可编译；若 Android target 出问题，可 `cfg(not(android))` 限制火山仅桌面（阶跃仍是默认）
- 不破坏阶跃：`provider=Stepfun` 路径走原 `submit_utterance`，回归测试

## 验证
- `cargo build`（桌面）通过
- 设置页切到火山，填 key/resource_id，启动 session，字幕正常流式显示
- 切回阶跃，回归正常
- 历史/搜索不受影响（DB 写入路径不变）

## 实施进度（2026-07-05）

### 已完成
- 依赖 `tokio-tungstenite`(rustls) + `flate2` 已加 `Cargo.toml`
- 配置层：`AsrProvider` enum + `AppConfig` 加 `provider`/`volc_api_key`/`volc_resource_id`/`volc_url`
- **`volc_model` 字段已移除** —— Resource ID 即模型；payload `model_name` 用固定常量 `bigmodel`（`DEFAULT_VOLC_MODEL`，保留在 `config.rs`）
- `asr/volc.rs`：二进制 WS 协议（4 字节 header + seq + gzip payload），full request + 分段 audio + last 包，响应解析 → `AsrEvent`
- `asr/provider.rs`：`AsrConfig` enum 分发 stepfun / volc
- pipeline/session 集成：`spawn` 接 `AsrConfig`；`session_start` 按 provider 构造并校验对应 key（volc 缺 key 返回 `VolcApiKeyMissing`）
- 配置读写：`config_set` mirror + `load_config` 加载新字段
- 前端：`settings.ts` + `Settings.vue` 加 ASR 服务下拉 + 火山配置（API Key / 模型·Resource ID / 高级 URL）
- 编译通过；5 个协议单测通过（header 编解码、gzip 往返、full/last/error 响应解析）
- 阶跃路径回归不受影响

### 已联调验证（2026-07-06）
1. ✅ **响应解析**：`extract_text` 走 `result.text` 路径正确，partial + final 均正常提取
2. ✅ **发送节奏**：快速连发 26~58 段无报错，`bigmodel_async` 不需要实时 pacing
3. ✅ **payload 配置**：`format:"wav"` 是 bug（服务端按 WAV 解码报 `invalid WAV file format` code=45000151）→ 改 `format:"pcm"` 后正常
4. ✅ **流式 partial**：服务端边收音频边返回增量结果（`audio_info.duration` 随上传增长），多条 `is_last=false` + 一条 `is_last=true`

### 踩坑记录
- **WebView2 缓存损坏**：多次强制中断 `npm run tauri dev` 会损坏 WebView2 user-data → 下次启动报 `WebView2 error 0x80070057 参数错误`（应用能跑、SQLite ready，但某窗口创建失败）。修复：删 `%LOCALAPPDATA%\com.realtimesubtitle.tool\EBWebView`（自动重建，`subtitle.db` 在 Roaming 不受影响）；并清理占 1420 端口的残留 `node.exe`/`msedgewebview2.exe`

### 启动 / 测试
```bash
cd /e/VibeCoding/realtimeText && HTTP_PROXY= HTTPS_PROXY= NO_PROXY="*" npm run tauri dev
```
设置页 → ASR 服务选「火山引擎 SAUC」→ 填 API Key + Resource ID（默认 `volc.seedasr.sauc.duration`）→ 保存 → 开始会话。

### 实际文件改动
**新增**：`src-tauri/src/asr/volc.rs`、`src-tauri/src/asr/provider.rs`
**修改**：`Cargo.toml`、`config.rs`、`commands/config.rs`、`lib.rs`、`asr/mod.rs`、`asr/pipeline.rs`、`commands/session.rs`、`stores/settings.ts`、`windows/dashboard/views/Settings.vue`

## 实时流式改造（2026-07-06）

### 问题
联调发现火山字幕"慢一步"：说完一整句停顿后字才刷出来。根因——pipeline 是**批处理模式**：VAD 攒整句（静音 800ms 断句）→ 才建连 → 才上传 → 才收 partial。火山 SAUC 本支持流式（边传边出字），但没利用。

### 方案（详见 `specs/volc-streaming-plan.md`）
**铁律：stepfun 路径一行不改**（`client.rs` 是 one-shot SSE，本就不支持流式上传，现状对它最优）。只改 volc + VAD 事件层。

- `vad/state.rs`：`push_frame` 返回 `Vec<VadEvent>`（`SpeechStart`/`SpeechFrame`/`SpeechEnd`），检测逻辑/常量零改，仅输出形态从整段 Utterance 变增量事件。4 单测更新断言。
- `asr/volc.rs`：删批处理 `submit_utterance_volc`，新增 `VolcStream{start,send_audio,finish}`。内部 `ws.split()` + `select!` 并发：边发音频段边收响应，partial 在上传过程中就流出。
- `asr/provider.rs`：删批处理 `submit_utterance`，留 `AsrConfig` 枚举。
- `asr/pipeline.rs`：`run_main` 发 `VadEvent`；`run_drainer` 按 `AsrConfig` 分两条——stepfun 重建整段调 `client::submit_utterance`（逐字节等价旧逻辑），volc 用 `VolcStream` 流式（攒 100ms 发一包，`VOLC_FINAL_TIMEOUT=20s` 防卡死）。

### 验证结果
日志证据（13:46:49~53）：多条 `volc resp is_last=false` 的 partial 在 `vad committed utterance` **之前**出现，`audio_info.duration` 从 3456ms→7168ms 随上传增长，文本边说边长（"这个智能体来加载 skill 自动帮"→"...技能包"→"...明白吗？是这个。"）。58 个 100ms 包，断句后 `finish` → `is_last=true` final。用户确认"效果很好"。16 个单测全绿。

### 实际文件改动（流式）
**修改**：`vad/state.rs`、`vad/mod.rs`、`asr/volc.rs`、`asr/provider.rs`、`asr/pipeline.rs`
**新增**：`specs/volc-streaming-plan.md`
**未动**：`asr/client.rs`（stepfun，零改动保证）
