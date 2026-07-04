# WebSocket 协议契约：阶跃星辰 StepAudio 2.5 Realtime

> **⚠️ 已废弃（2026-06-30）**：本契约描述的 WebSocket 双向流式方案因成本过高（1.2 元/小时）已弃用。当前实现改用 SSE 一次性接口（0.15 元/小时）+ 本地 silero-vad 切句，新协议见 [sse-protocol.md](./sse-protocol.md)。本文档保留作为历史参考与未来回退依据。

**Date**: 2026-06-30
**Status**: 已废弃
**Direction**: 客户端 ↔ 服务端，全双工

**Endpoint**: `wss://api.stepfun.com/v1/realtime/asr/stream`

**鉴权**: HTTP Upgrade 请求头 `Authorization: Bearer $STEPFUN_API_KEY`

---

## 客户端 → 服务端

### 1. `session.update`（连接建立后立即发送一次）

```json
{
  "type": "session.update",
  "session": {
    "audio": {
      "input": {
        "format": {
          "type": "pcm",
          "codec": "pcm_s16le",
          "rate": 16000,
          "bits": 16,
          "channel": 1
        }
      }
    }
  }
}
```

### 2. `input_audio_buffer.append`（持续推送音频帧）

```json
{
  "type": "input_audio_buffer.append",
  "audio": "<base64-encoded 1280-byte pcm_s16le>"
}
```

**频率**: 每 40ms 一帧（16000Hz × 0.04s × 2B = 1280 字节 PCM → Base64 后约 1708 字符）。

### 3. `session.close`（用户主动停止时发送）

```json
{ "type": "session.close" }
```

---

## 服务端 → 客户端

### A. 中间非稳态结果（partial）

随模型前向推理持续修正，文本动态变化。前端用于滚动草稿。

```json
{
  "type": "response.audio_transcript.partial",
  "transcript": "正在变化的草稿文本",
  "start_ts": 1719700000000
}
```

### B. 稳态结果（final）

VAD 检测到句尾静音时下发，标志当前段落识别周期终结。

```json
{
  "type": "response.audio_transcript.final",
  "transcript": "最终确定的文本",
  "start_ts": 1719700000000,
  "end_ts": 1719700035000,
  "paralinguistic": {
    "emotion": "neutral",
    "speech_rate": "normal",
    "non_verbal": []
  }
}
```

### C. 错误事件

```json
{
  "type": "error",
  "code": "auth_failed | rate_limit | internal",
  "message": "人类可读错误描述"
}
```

**错误处理**:
- `auth_failed`：API Key 无效，停止重连，触发 FR-013 引导用户重新配置
- `rate_limit`：指数退避延长至 60s 起步
- `internal`：按默认重连策略处理

---

## 重连策略（客户端实现）

| 尝试次数 | 等待时长 |
| --- | --- |
| 1 | 1s |
| 2 | 2s |
| 3 | 4s |
| 4 | 8s |
| 5 | 16s |
| 6+ | 30s（封顶） |

- 断连期间音频在内存环形缓冲暂存（上限 30s）
- 重连成功后批量补发 buffered 帧
- 超过 30s 未重连则丢弃缓冲，发 `asr-status` 提示用户

---

## 备注

- 上述消息字段名基于阶跃星辰实时 ASR 流式 API 文档（架构文档 #16）的典型形态。**实际字段名以官方文档为准**，实现阶段需对照 `https://platform.stepfun.com/docs/zh/api-reference/audio/asr-stream` 校准。
- partial/final 事件的具体 `type` 字符串可能为 `response.audio_transcript.partial`/`.final` 或其他变体，实现时验证。
