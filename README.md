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

```bash
# 安装前端依赖
npm install

# 开发模式（同时启动 vite 和 tauri）
npm run tauri dev

# 构建 Windows 安装包 (MSI / NSIS)
npm run tauri build
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
