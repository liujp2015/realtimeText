# Phase 0 Research: ort on Android + Toolchain

**Date**: 2026-07-05 | **Phase**: P0 (research/spike)

## HEAD RISK 验证：ort 2.0.0-rc.12 on Android aarch64

### 结论（spike 实测 2026-07-05 ✅ 通过）

**spike 验证**：独立最小项目 `spike-ort-android/`，`cargo ndk --target aarch64-linux-android build` 成功（6.15s，exit 0）。ort 2.0.0-rc.12 可交叉编译到 Android aarch64。

**关键发现**：必须同时启用 `load-dynamic` + `api-24`。
- 单用 `load-dynamic`（`default-features = false`）会丢掉 `api-24`，ort-sys 退回低版本绑定，`OrtApi` 缺 `SessionOptionsAppendExecutionProvider_VitisAI` 字段，`src/ep/vitis.rs:47` 在 `#[cfg(any(feature = "load-dynamic", feature = "vitis"))]` 块下编译失败（E0609）。
- `api-24` 对应 onnxruntime 1.24 绑定（含该字段）。`default` feature 本就含 `api-24`，故桌面 `download-binaries` 模式不报错。
- 正确配置：`ort = { version = "2.0.0-rc.12", default-features = false, features = ["load-dynamic", "api-24"] }`。

- ort 2.0.0-rc.12 对应 **ONNX Runtime 1.24**（.so 版本必须匹配）。
- `download-binaries` feature 对 Android aarch64 **不可靠**：cdn.pyke.io 的 `aarch64-linux-android` 二进制截至 2025-12 仍标注 "device-unverified"。**不采用。**
- 推荐路径：**`load-dynamic` feature + 从官方 onnxruntime-android AAR 提取 libonnxruntime.so 放入 jniLibs**。
  - AAR（`com.microsoft.onnxruntime:onnxruntime-android:1.24`）本质是 ZIP，解压后 `jni/<abi>/libonnxruntime.so`。
  - ABI 映射：`aarch64-linux-android`→`arm64-v8a`，`armv7-linux-androideabi`→`armeabi-v7a`，`x86_64-linux-android`→`x86_64`。
  - 放到 Android 工程的 `src/main/jniLibs/<abi>/libonnxruntime.so`。
  - Rust 侧：`ort = { version = "2.0.0-rc.12", default-features = false, features = ["load-dynamic"] }`，运行时通过 `ORT_DYLIB_PATH` 或 Android 原生库加载找到 .so。
  - **Android dlopen 细节待验证**：Android 7+ 限制 dlopen 命名空间，`load-dynamic` 在 Android 可能需额外配置（显式指定 .so 全路径，或依赖 `System.loadLibrary` 预加载）。这是 spike 的核心验证点。

### 备选方案（若 load-dynamic 在 Android 受阻）

1. 从源码编译 ONNX Runtime for Android（cmake + NDK，可启用 NNAPI/XNNPACK EP），产物 `libonnxruntime.so` 同样放 jniLibs。复杂度高、编译慢，但可控。
2. 弃用 Rust ort，改用 ONNX Runtime 官方 Android Java API（Kotlin 侧直接调 onnxruntime-android）。代价：VAD 推理从 Rust 移到 Kotlin，破坏核心复用。仅作最后手段。

### Cargo.toml 预期改动（P2）

```toml
# 桌面：download-binaries；Android：load-dynamic
[target.'cfg(not(target_os = "android"))'.dependencies]
ort = { version = "2.0.0-rc.12", features = ["download-binaries"] }

[target.'cfg(target_os = "android")'.dependencies]
ort = { version = "2.0.0-rc.12", default-features = false, features = ["load-dynamic"] }
```

（精确语法 spike 验证。）

## 工具链状态（2026-07-05）

| 组件 | 状态 |
|---|---|
| Tauri CLI 2.11.4（含 android 子命令） | ✅ |
| Rust 1.94 / cargo 1.94 | ✅ |
| rustup Android targets | ⏳ 安装中（下载 rust-std） |
| Android SDK | ❌ 未安装 |
| Android NDK | ❌ 未安装 |
| JDK | ⚠️ 21（官方要求 17，待验证） |
| cargo-ndk | ❌ 未装（spike 需要） |
| ANDROID_HOME / NDK_HOME 环境变量 | ❌ 全空 |

### 待装清单（spike 前置）

1. Android SDK（platform-tools + cmdline-tools）
2. Android NDK（Tauri Android 默认 NDK r26d 左右）
3. `cargo install cargo-ndk`（简化交叉编译 + jniLibs 结构）
4. JDK 17 验证 / 降级
5. rustup Android targets（进行中）
6. onnxruntime-android AAR 1.24（提取 .so）

## 其他 Phase 0 发现

- `lib.rs:84` `device_monitor` 调用 cpal 的 `current_default_output_name()`，mobile 编译会失败 → P1 需 `#[cfg(not(target_os = "android"))]` 或平台分支。
- `tauri.conf.json` 桌面双窗口 → Android 需 `tauri.android.conf.json` 单窗口覆盖（P2）。
- cpal 0.15 在 Android 有 stub，但 AudioPlaybackCapture 需 Kotlin 侧实现 → P1 AudioSource trait 抽象隔离 cpal。

## 下一步（用户回来后）

1. 确认 SDK/NDK 安装方式（命令行 / Android Studio / 已有）
2. 确认 JDK 处理（先用 21 试 / 装 17）
3. 装齐工具链后，跑 ort load-dynamic spike：`cargo ndk build --target aarch64-linux-android` 一个最小 ort 程序，验证编译 + 链接
4. 真机 AudioPlaybackCapture demo（需真机，模拟器不可靠）

## 来源

- ort crate 文档（crates.io / docs.rs）— download 策略与 execution providers
- ort GitHub discussions 2025-12 — aarch64-linux-android 预编译 "device-unverified"
- onnxruntime-android Maven AAR — jni/<abi>/libonnxruntime.so 结构
