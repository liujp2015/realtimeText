# IPC 事件契约：Tauri v2 Emitter

**Date**: 2026-06-30
**Direction**: Rust → 前端（广播）

Rust 后端通过 `AppHandle::emit(event, payload)` 广播事件；前端通过 `@tauri-apps/api/event` 的 `listen(event, handler)` 订阅。

---

## `subtitle-update`

单句字幕更新，可能是动态草稿（partial）或定格最终句（final）。

```typescript
type SubtitleUpdate = {
  state: "partial" | "final";
  text: string;              // 当前文本
  start_ts: number;          // 起始 Unix 时间戳（毫秒）
  end_ts?: number;           // 仅 final 时存在
  paralinguistic?: {
    emotion?: "neutral" | "happy" | "sad" | "angry" | "frustrated" | "surprised";
    speech_rate?: "slow" | "normal" | "fast";
    non_verbal?: string[];   // ["laugh", "sigh", "cough"]
  };
};
```

**前端行为**:
- `partial`：替换当前草稿行内容，滚动显示
- `final`：把草稿行归档至历史区，清空草稿行，下一条 partial 接续

**频率**: partial 可能每秒多次；final 由 VAD 句尾触发，约每 3–10 秒一次。

---

## `session-meta`

会话启动通知，前端据此创建会话上下文。

```typescript
type SessionMeta = {
  session_guid: string;      // UUID v4
  started_at: number;        // Unix 时间戳（秒）
  device_name: string;       // 当前捕获设备名
};
```

**触发时机**: 用户点击「开始字幕」并成功建立 WS 连接后，由 Rust 一次性广播。

---

## `asr-status`

识别服务连接状态变化通知。

```typescript
type AsrStatus = {
  connected: boolean;
  retry_count: number;       // 累计重试次数，连接成功后归零
  last_error?: string;       // 最近错误信息（用户可见）
};
```

**触发时机**:
- 连接建立成功：`{ connected: true, retry_count: 0 }`
- 连接断开：`{ connected: false, retry_count: N, last_error: "..." }`
- 每次重连尝试：更新 `retry_count`

前端应在前端显眼位置（如字幕窗角落或托盘菜单）展示连接状态图标。

---

## 事件监听器生命周期约束

前端 `listen` 返回 `Promise<UnlistenFn>`。组件卸载时必须：

```typescript
onMounted(async () => {
  const unlisten = await listen<SubtitleUpdate>("subtitle-update", handler);
  onBeforeUnmount(() => unlisten());
});
```

**禁止**：在 `listen` Promise 未 resolve 前同步调用 unlisten（会得到 undefined，导致泄漏）。
