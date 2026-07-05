# Implementation Plan: Android Port of Realtime Subtitle Tool

**Branch**: `002-android-port` | **Date**: 2026-07-05 | **Parent**: [001 plan](../001-realtime-subtitle-tool/plan.md)

> 计划于 2026-07-04 批准（inline），本文档为正式化版本。**当前进度（2026-07-05）：P1+P2 完成** — Phase 0 工具链就绪 + ort spike 通过；P1 AudioSource 平台抽象（桌面零回归）；P2 Android 脚手架（主项目 `cargo ndk build` aarch64 通过）。下一步 P3（MIC 全链路 + 真机）。

## Summary

将 001 的 Windows 实时字幕工具移植到 Android。复用 Rust 核心（DSP/VAD/ASR/DB/pipeline），重写音频采集层（cpal WASAPI → Android JNI AudioPlaybackCapture + MIC）与字幕渲染层（Tauri 双窗口 → 单 WebView dashboard + 原生 Kotlin overlay）。

## 已批准的产品决策（2026-07-04）

1. **音频采集**：AudioPlaybackCapture 为主 + MIC 兜底。
   - Android 10+ `AudioPlaybackCaptureConfiguration` via MediaProjection 捕获大多数现代播放器（targetSdk≥29 默认 `allowAudioPlaybackCapture=true`）。
   - 对 DRM 内容 / 通话 / opt-out 应用，回退到 `AudioRecord(MIC)`。
2. **字幕显示**：原生 Kotlin overlay（`WindowManager TYPE_APPLICATION_OVERLAY`），跨应用悬浮字幕。**不**用 WebView 渲染字幕。前台服务保活。

## 可复用的 Rust 核心（零/最小改动）

- `audio/dsp.rs`、`audio/ring.rs`：纯 DSP，平台无关。
- `vad/silero.rs`：仅 ort feature 改动（Android 后端）。
- `vad/state.rs`：状态机，平台无关。
- `asr/*`：reqwest + rustls，跨平台。
- `db/*`：`app_data_dir` 在 Android 解析为 app 私有目录。
- `commands/*`：`session_start/stop` 需平台分支。
- Pipeline 已解耦，只消费 `Consumer<f32>` + `source_rate`。
- `lib.rs` 已有 `#[cfg_attr(mobile, tauri::mobile_entry_point)]`。

## 需重写/新增

- **音频采集层**：cpal → Android JNI。Kotlin 驱动采集线程，PCM 经 `push_samples` JNI 推入 Rust `Producer`。
- **字幕渲染**：Tauri 双窗口 → 单 WebView dashboard + 原生 overlay。
- **ort Android 后端**：平台条件依赖。
- **反向 JNI**：Rust → Kotlin overlay `onSubtitle`。

## 6 阶段计划（~12-17 天）

- **P0** research/spike（1-2d）：ort Android 构建（**HEAD RISK**）、Tauri Android 工具链、AudioPlaybackCapture 真机 demo。
- **P1** AudioSource trait 抽象 + 桌面零回归（0.5d）。
- **P2** Android 脚手架，tauri.conf mobile 单窗口，ort 平台条件依赖（1-2d）。
- **P3** MIC 路径全链路跑通（2-3d）。
- **P4** AudioPlaybackCapture + MIC 兜底逻辑（2-3d）。
- **P5** 原生 overlay + 反向 JNI + 前台服务（3-4d）。
- **P6** 权限 UX、生命周期、签名 APK（2d）。

## Phase 0 状态（2026-07-05）

### 工具链检查结果（2026-07-05 全部就绪）

| 组件 | 状态 |
|---|---|
| Tauri CLI 2.11.4（含 `android` 子命令） | ✅ |
| Rust 1.94 / cargo 1.94 | ✅ |
| Java | ⚠️ JDK 21（`D:\Program Files\Java\jdk-21.0.11`；尚未对 Gradle 验证，P2 实测失败再降 17） |
| rustup Android targets | ✅ aarch64 / armv7 / x86_64 |
| Android SDK | ✅ cmdline-tools + platform-tools + platforms;android-34 + build-tools;34.0.0 |
| Android NDK | ✅ 27.2.12479018 |
| cargo-ndk | ✅ 4.1.2 |
| ANDROID_HOME / NDK_HOME 环境变量 | ✅ 持久化 |

### Phase 0 步骤

1. ✅ 工具链检查
2. ✅ 安装 rustup Android targets
3. ✅ 安装 Android SDK + NDK（命令行 sdkmanager）
4. ⏳ 验证 JDK 21 对 Gradle（P2 `tauri android init` 时实测，失败再降 17）
5. ✅ ort 2.0.0-rc.12 spike **通过**：`cargo ndk --target aarch64-linux-android build` 成功（6.15s）。关键：`features = ["load-dynamic", "api-24"]`——单 `load-dynamic` 会因 ort-sys 低版本绑定缺 `SessionOptionsAppendExecutionProvider_VitisAI` 字段、`vitis.rs` 编译失败。详见 research.md。
6. ⏳ AudioPlaybackCapture 真机 demo（需真机，模拟器不可靠）

## 风险

- ort Android NDK 链接（P0，HEAD RISK）
- 反向 JNI 线程 attach（P5）
- AudioPlaybackCapture 真机行为（P0/P4）
- 前台服务被杀（P5）
- Tauri mobile 单窗口 vs desktop 双窗口配置（P2）
