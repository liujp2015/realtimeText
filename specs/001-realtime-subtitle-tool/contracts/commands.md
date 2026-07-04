# Tauri Command 契约

**Date**: 2026-06-30
**Direction**: 前端 → Rust（通过 `@tauri-apps/api/core` 的 `invoke`）

Rust 后端通过 `#[tauri::command]` 宏暴露命令；前端通过 `invoke("command_name", { args })` 调用。

---

## 会话管理

### `session_start`

启动字幕识别：建立 WS 连接、初始化会话、启动音频捕获。

```typescript
invoke("session_start"): Promise<{
  session_guid: string;
  started_at: number;
  device_name: string;
}>;
```

**错误**:
- `ApiKeyMissing`：未配置 API Key
- `NoOutputDevice`：系统无可用输出设备
- `AlreadyRunning`：已有会话运行中

### `session_stop`

停止当前会话：关闭 WS、释放音频流、写入 `ended_at`。

```typescript
invoke("session_stop"): Promise<void>;
```

### `session_list`

分页拉取历史会话列表。

```typescript
invoke("session_list", { limit: number, offset: number }): Promise<{
  total: number;
  items: Array<{
    guid: string;
    started_at: number;
    ended_at: number | null;
    device_name: string;
    transcription_count: number;
  }>;
}>;
```

### `session_get`

拉取指定会话的完整字幕时间轴。

```typescript
invoke("session_get", { guid: string }): Promise<{
  guid: string;
  started_at: number;
  ended_at: number | null;
  device_name: string;
  transcriptions: Array<{
    id: number;
    text: string;
    start_ts: number;
    end_ts: number;
    paralinguistic: object | null;
  }>;
}>;
```

### `session_delete`

删除指定会话及其全部字幕（级联）。

```typescript
invoke("session_delete", { guid: string }): Promise<void>;
```

### `history_clear`

清空全部历史（删除所有会话与字幕）。

```typescript
invoke("history_clear"): Promise<void>;
```

---

## 检索

### `search_keywords`

跨会话关键词检索。

```typescript
invoke("search_keywords", { keyword: string }): Promise<{
  items: Array<{
    id: number;
    session_guid: string;
    session_started_at: number;
    text: string;
    start_ts: number;
    end_ts: number;
  }>;
}>;
```

**约束**: 最多返回 200 条，按 `start_ts DESC` 排序。

---

## 配置

### `config_get`

读取配置项。

```typescript
invoke("config_get", { key: string }): Promise<unknown | null>;
```

支持键：`api_key`、`appearance`、`window`。

### `config_set`

写入配置项。

```typescript
invoke("config_set", { key: string, value: unknown }): Promise<void>;
```

**校验**:
- `appearance.font_size` ∈ [12, 72]
- `appearance.bg_opacity` ∈ [0.0, 1.0]
- `api_key` 非空字符串（允许空串以表示清除，但启动识别时会再次校验）

### `config_reset_appearance`

重置样式为出厂默认。

```typescript
invoke("config_reset_appearance"): Promise<{
  font_family: string;
  font_size: number;
  text_color: string;
  bg_opacity: number;
}>;
```

---

## 错误约定

所有 command 失败时 `invoke` 的 Promise reject 一个对象：

```typescript
{
  code: string;       // 机器可读错误码，如 "ApiKeyMissing"
  message: string;    // 人类可读描述，可直显
}
```

前端应统一捕获并展示 `message`，避免静默失败（呼应 FR-013）。
