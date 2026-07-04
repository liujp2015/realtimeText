# Realtime Subtitle Tool (Windows)

基于 Tauri v2 + 阶跃星辰 StepAudio 2.5 Realtime 的 Windows 桌面实时字幕工具。
捕获系统播放器输出音频，经端到端语音大模型流式识别，悬浮透明窗渲染字幕，本地 SQLite 持久化历史。

## 平台

- 仅支持 Windows 10 / Windows 11 (x64)
- 不采集麦克风，仅捕获系统播放器回环音频 (WASAPI Loopback)

## 前置条件

- [Rust 1.75+](https://rustup.rs/)
- [Node.js 20+](https://nodejs.org/)
- [阶跃星辰 StepAudio API Key](https://platform.stepfun.com/)

## 开发

### ⚠️ 代理设置（重要）

本机系统代理 `https_proxy=127.0.0.1:7890` 通常处于关闭状态，若不覆盖会导致 cargo / npm / API 请求全部卡死。
**所有 cargo 与 npm 命令都必须关闭代理**：国内直连 crates.io sparse index、npmmirror、`api.stepfun.com` 均可达。

在每次命令前显式覆盖环境变量（Git Bash / WSL 语法）：

```bash
HTTP_PROXY= HTTPS_PROXY= NO_PROXY="*" <你的命令>
```

### 首次安装

```bash
# 安装前端依赖（同样需要关闭代理）
HTTP_PROXY= HTTPS_PROXY= NO_PROXY="*" npm install
```

### 开发模式

```bash
# 启动 vite 前端 (localhost:1420) + tauri 后端（首次 cargo 编译约 40-60s）
HTTP_PROXY= HTTPS_PROXY= NO_PROXY="*" npm run tauri dev
```

启动成功标志：
- Vite 就绪：`http://localhost:1420/`
- cargo 输出 `Finished dev profile ...` 并启动 `target\debug\realtime-subtitle-tool.exe`
- 日志出现 `sqlite pool ready at ...subtitle.db`
- 字幕悬浮窗与设置面板自动弹出（dev 模式字幕窗自动开 DevTools）

> **HMR**：前端改动由 Vite 热更新，无需重启；Rust 改动会自动触发 cargo 重编译并重启 exe。
>
> **exe 被占用**：若上次进程未正常退出导致 `target\debug\realtime-subtitle-tool.exe` 被锁，
> 先用任务管理器结束 `realtime-subtitle-tool.exe` 进程再重新启动。

### 构建发布包

```bash
# 构建 Windows 安装包 (MSI / NSIS)
HTTP_PROXY= HTTPS_PROXY= NO_PROXY="*" npm run tauri build
```

## 使用

1. 启动应用，进入「设置」填入阶跃星辰 API Key
2. 点击右上角「开始字幕」
3. 在任意播放器播放带人声内容，字幕将在屏幕上方悬浮显示
4. 鼠标点击穿透字幕窗到达底层播放器
5. 拖动字幕窗右上角手柄可移动位置
6. 进入「历史」查看过往会话与关键词检索

## 限制说明

- **DRM 内容**：受 DRM 保护的音频流（如 Netflix）无法被回环捕获，字幕窗将提示
- **多播放器混音**：同时开启多个播放器会被合并识别
- **设备切换**：系统默认输出设备切换时，需停止并重新开始字幕以绑定新设备（v1 限制）

## 架构

详见 `specs/001-realtime-subtitle-tool/` 下的 spec / plan / research / data-model / contracts / tasks。

## 数据与隐私

- 全部历史数据存储在 `%APPDATA%\com.realtimesubtitle.tool\`
- 数据库：`subtitle.db`
- 日志：自动轮转，单文件 ≤ 50KB
- 音频数据仅发送至阶跃星辰识别服务，不上传任何第三方
