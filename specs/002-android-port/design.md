# P1/P2 架构设计:Android 采集 + 渲染

**Date**: 2026-07-05 | 基于 Phase 0 调研 + 当前代码结构

## 核心架构差异:桌面 vs Android

| 维度 | 桌面(现状) | Android(目标) |
|---|---|---|
| 采集驱动 | Rust 主动(cpal 线程) | Kotlin 主动(AudioRecord 线程)→ JNI push 到 Rust |
| 采集依赖 | cpal 0.15 | Kotlin AudioPlaybackCapture / AudioRecord(MIC) |
| 字幕渲染 | Tauri 第二窗口(subtitle) | native overlay(`TYPE_APPLICATION_OVERLAY`) |
| Rust→UI | `emit("subtitle-update")` 到 webview | Tauri plugin `@Command onSubtitle` → Kotlin overlay |
| 窗口数 | 2(subtitle + dashboard) | 1(dashboard only) |

**关键不变量**:pipeline 只消费 `Consumer<f32>` + `source_rate`(已解耦),两端复用 `dsp/vad/asr/db` 全部 Rust 核心。

## P1:AudioSource 平台抽象(桌面零回归)

### 变更点

1. **`Cargo.toml`**:cpal 限桌面 only(Android 不编译 cpal,避免 stub 问题)
   ```toml
   [target.'cfg(not(target_os = "android"))'.dependencies]
   cpal = "0.15"
   ```
2. **`audio/capture.rs`**:整个模块 `#[cfg(not(target_os = "android"))]` 门控。桌面保留 `start_loopback_thread`。
3. **`audio/mod.rs`**:新增 Android 分支占位(空模块或 JNI push 入口)。
4. **`lib.rs`**:`device_monitor` 调 `current_default_output_name()`(cpal),需 `#[cfg(not(target_os = "android"))]` 门控整个 `device_monitor` 或其调用点。
5. **`commands/session.rs`**:`session_start` 里 `start_loopback_thread` 调用需平台分支。抽象为统一入口:
   ```rust
   // audio/mod.rs
   pub fn start_audio_source(producer: Producer<f32>)
       -> (JoinHandle<()>, mpsc::Sender<()>, mpsc::Receiver<Result<CaptureInfo, String>>);
   ```
   桌面:委托 `start_loopback_thread`。Android:见 P3(被动模式,返回的 info_rx 由 Kotlin 回填 sample_rate)。

### 验证
- 桌面 `cargo build` 零回归(11 单测仍过)。
- `cargo ndk --target aarch64-linux-android build` 主项目能过 cpal 门控(ort 配置见下)。

## P2:Android 脚手架 + ort 平台条件依赖

### 变更点

1. **`Cargo.toml`**:ort 平台条件依赖(Phase 0 spike 结论)
   ```toml
   [target.'cfg(not(target_os = "android"))'.dependencies]
   ort = { version = "2.0.0-rc.12", features = ["download-binaries"] }

   [target.'cfg(target_os = "android")'.dependencies]
   ort = { version = "2.0.0-rc.12", default-features = false, features = ["load-dynamic", "api-24"] }
   ```
   - jni = "0.21"(Android JNI 绑定,`extern "system"` 导出 push_samples)
2. **`tauri.android.conf.json`**(新建,merge 覆盖):windows 只留 dashboard
   ```json
   { "app": { "windows": [ { "label": "dashboard", "url": "dashboard.html", ... } ] } }
   ```
3. **`tauri android init`**:生成 `src-tauri/gen/android/` Kotlin 工程。验证 NDK 27.2 兼容(若 Tauri 报版本不符,补装它要求的 NDK)。
4. **libonnxruntime.so**:从 `onnxruntime-android` AAR 1.24(对应 ort rc.12)解压 `jni/arm64-v8a/libonnxruntime.so` → `src-tauri/gen/android/app/src/main/jniLibs/arm64-v8a/`。同样放 armeabi-v7a、x86_64。

### 验证
- `tauri android init` 成功生成 Kotlin 工程。
- 主项目 `cargo ndk --target aarch64-linux-android build` 通过(ort load-dynamic + cpal 门控)。
- 运行时 dlopen libonnxruntime.so:Android 7+ dlopen 命名空间风险点——若 `load-dynamic` 找不到 .so,需显式 `ORT_DYLIB_PATH` 或 `System.loadLibrary` 预加载(P3 实测)。

## P3:MIC 路径全链路(Android)

先 MIC(简单,不需 MediaProjection),验证 pipeline 端到端。

### 启动机制决策:Tauri mobile plugin

Rust `session_start` 需触发 Kotlin 启动 `AudioCaptureService`。采用 **Tauri mobile plugin**(P5 overlay 也复用此通道):
- Rust 侧:定义 `audio_capture` plugin(注册到 `tauri::Builder`),`session_start` Android 分支经 `PluginHandle` 调 Kotlin `startCapture`。
- Kotlin 侧:`AudioCapturePlugin.kt`(`app.tauri.plugin.Plugin` + `@Command startCapture/stopCapture`),启动/停止 `AudioCaptureService`。
- `.so` 加载:`AudioBridge.kt` `System.loadLibrary` 声明 `external pushSamples/notifyCaptureInfo`,Rust 导出 `Java_com_realtimesubtitle_tool_AudioBridge_*` JNI 符号。
- 全局 producer:Rust 侧 `static` 持有 `Arc<Producer>`,`session_start` 写入、`pushSamples` JNI 读取。

### 采集架构(被动模式)

```
Kotlin AudioCaptureService(MIC)
  → AudioRecord.read(ByteBuffer)  // PCM s16le, 16k mono
  → JNI: processAudioSamples(rustPtr, buffer, size)
Rust #[no_mangle] Java_..._processAudioSamples
  → bytes → f32 → producer.push()  // producer 存于 AppState
  → pipeline(已存在)消费 consumer
```

### Rust 侧变更

1. **`state.rs`**:`AppState` 加 Android 字段 `audio_producer: Option<Arc<Mutex<Producer<f32>>>>`(供 JNI push)。
2. **新模块 `audio/jni.rs`**(`#[cfg(target_os = "android")]`):
   ```rust
   #[no_mangle]
   pub extern "system" fn Java_com_realtimesubtitle_tool_..._pushSamples(
       env: JNIEnv, _class: JClass, samples: JByteArray
   ) {
       // 取 AppState.audio_producer,bytes→f32,push
   }
   #[no_mangle]
   pub extern "system" fn Java_..._notifyCaptureInfo(env, _class, sample_rate: jint, device_name: JString) {
       // 回填 CaptureInfo(解决 Android sample_rate 由 Kotlin 决定)
   }
   ```
3. **`commands/session.rs`** `session_start` Android 分支:
   - 创建 ring,consumer→pipeline,producer 存入 AppState。
   - 调 Kotlin plugin command `startCapture(MIC)` 启动 AudioCaptureService。
   - CaptureInfo 由 Kotlin 回填 `notifyCaptureInfo`。

### Kotlin 侧(P3 用 MIC,P4 加 AudioPlaybackCapture)
- `AudioCaptureService`:foreground service,AudioRecord(MIC) 16k mono s16le,循环 read → JNI `pushSamples`。
- 包名对齐 JNI 符号:`com.realtimesubtitle.tool`(已在 tauri.conf identifier)。

### 验证
- 真机:启动 → MIC 采集 → VAD 切句 → SSE ASR → 字幕(先 logcat 确认,overlay P5)。

## P4:AudioPlaybackCapture + MIC 兜底

- `AudioCaptureService` 扩展:MediaProjection consent → `AudioPlaybackCaptureConfiguration.Builder(mediaProjection).addMatchingUsage(USAGE_MEDIA/GAME/UNKNOWN)` → AudioRecord.Builder.setAudioPlaybackCaptureConfig。
- 权限:RECORD_AUDIO、FOREGROUND_SERVICE、FOREGROUND_SERVICE_MEDIA_PROJECTION(Android 14+)。
- 兜底逻辑:DRM/通话/opt-out 应用 → AudioPlaybackCapture 返回静音/无数据 → 检测后切 MIC。
- targetSdk≥29(默认 allowAudioPlaybackCapture=true)。

## P5:native overlay + 反向 JNI + 前台服务

### Rust→Kotlin overlay(经 Tauri plugin)

1. **Tauri mobile plugin**(Kotlin):class extends `app.tauri.plugin.Plugin`,`@Command onSubtitle(text: String, state: String)` → 更新 `TYPE_APPLICATION_OVERLAY` 的 TextView。
2. **`events.rs`** `emit_subtitle_update` Android 分支:不 emit webview,改调 plugin `onSubtitle`(`PluginHandle::run_mobile_plugin`)。
3. **overlay window**:`WindowManager.addView(TextView, LayoutParams(TYPE_APPLICATION_OVERLAY))`,前台服务保活。
4. 权限:`SYSTEM_ALERT_WINDOW`(overlay)+ 引导用户授权。

### 反向 JNI 线程 attach
- Kotlin AudioCaptureService 线程调 JNI `pushSamples`:Rust JNI 函数无需 attach(`JNIEnv` 由调用方传入)。
- Rust 主动调 Kotlin(emit subtitle 经 plugin):Tauri plugin 机制处理线程。

## P6:权限 UX、生命周期、签名 APK

- 权限流:RECORD_AUDIO → MediaProjection consent → SYSTEM_ALERT_WINDOW。
- 生命周期:app 后台保活(foreground service),屏幕旋转/切后台处理。
- 签名 APK:`tauri android build`,签名配置。

## 变更点汇总(按文件)

| 文件 | 阶段 | 变更 |
|---|---|---|
| `Cargo.toml` | P1/P2 | cpal 桌面 only;ort 平台条件;加 jni 依赖(android) |
| `audio/capture.rs` | P1 | `#[cfg(not(target_os="android"))]` 门控 |
| `audio/mod.rs` | P1 | `start_audio_source` 统一入口 + 平台分支 |
| `audio/jni.rs` | P3 | 新建,`pushSamples`/`notifyCaptureInfo` JNI 导出(android) |
| `lib.rs` | P1 | `device_monitor` cfg-gate |
| `commands/session.rs` | P1/P3 | `session_start` 平台分支 |
| `state.rs` | P3 | AppState 加 audio_producer(android) |
| `events.rs` | P5 | `emit_subtitle_update` Android 分支(plugin) |
| `tauri.android.conf.json` | P2 | 新建,单窗口 |
| `tauri.conf.json` | P2 | identifier 已是 com.realtimesubtitle.tool ✅ |
| Kotlin plugin + Service | P3-P5 | 新建(gen/android/) |
| `jniLibs/<abi>/libonnxruntime.so` | P2 | 从 AAR 1.24 解压 |
