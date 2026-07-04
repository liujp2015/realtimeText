# SSE 协议契约：阶跃星辰 StepAudio 2.5 ASR（一次性提交 + 流式返回）

**Date**: 2026-06-30
**Direction**: 客户端 → 服务端 HTTP POST，服务端 → 客户端 SSE 流式响应
**替代**: `ws-protocol.md`（WebSocket 双向流式方案已废弃，因成本过高）

**背景**：原方案使用 WebSocket 流式接口（`stepaudio-2.5-asr-stream`，1.2 元/小时）。为降低成本改用 SSE 一次性接口（`stepaudio-2.5-asr`，0.15 元/小时），由本地 silero-vad 按句切分后逐句提交。代价是延迟从近实时变为「句子结束 + SSE 往返」（典型 1-3s）。

---

## 服务地址

`POST https://api.stepfun.com/step_plan/v1/audio/asr/sse`

> **注意**：Step Plan 套餐用户使用 `/step_plan/v1/` 前缀。标准用户使用 `https://api.stepfun.com/v1/audio/asr/sse`。当前实现硬编码为 Step Plan 端点（`src/asr/protocol.rs:3`）。

## 鉴权

HTTP 请求头 `Authorization: Bearer $STEPFUN_API_KEY`

---

## 请求

### Headers

| Header | 值 | 必填 |
| --- | --- | --- |
| `Content-Type` | `application/json` | 是 |
| `Accept` | `text/event-stream` | 是 |
| `Authorization` | `Bearer $STEPFUN_API_KEY` | 是 |

### Body

```json
{
  "audio": {
    "data": "<base64-encoded pcm_s16le>",
    "input": {
      "transcription": {
        "language": "zh",
        "model": "stepaudio-2.5-asr",
        "enable_itn": true
      },
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
```

**字段说明**：

- `audio.data` — Base64 编码的 PCM 音频数据（由本地 VAD 切句后的一个 utterance）
- `audio.input.transcription.language` — 识别语言，固定 `zh`
- `audio.input.transcription.model` — 模型名，固定 `stepaudio-2.5-asr`
- `audio.input.transcription.enable_itn` — 是否开启 ITN 文本规范化，默认 `true`
- `audio.input.format` — 音频格式，固定 PCM 16kHz/16bit/单声道

**音频来源**：每个请求对应本地 silero-vad 检测到的一个语音段（utterance），典型时长 1-10s，PCM 大小约 32KB-320KB。

---

## 响应

SSE 流式响应，事件以 `\n\n` 分隔，每个事件形如：

```
data: {"type":"transcript.text.delta","delta":"...","start_time":0,"end_time":500}
```

### 1. Delta 事件（`transcript.text.delta`）

增量转录文本，客户端应累积展示。

```json
{
  "type": "transcript.text.delta",
  "meta": {
    "session_id": "sse_xxx",
    "timestamp": 1642694400123
  },
  "delta": "识别的",
  "item_id": "item_xxx",
  "content_index": 0,
  "start_time": 0,
  "end_time": 500
}
```

- `delta` — 本次增量文本（非累计）。客户端需自行累积拼接。
- `start_time` / `end_time` — 该段文本在音频中的相对时间位置（毫秒）。

**客户端处理**：累积 `delta` 为完整文本，每次 delta 到达时 emit `subtitle-update {state:"partial", text:累积文本}`。

### 2. Done 事件（`transcript.text.done`）

完整转录文本已生成，标志本次 utterance 识别结束。

```json
{
  "type": "transcript.text.done",
  "meta": {
    "session_id": "sse_xxx",
    "timestamp": 1642694400456
  },
  "text": "识别的完整文字内容",
  "usage": {
    "type": "realtime_asr",
    "input_tokens": 1000,
    "output_tokens": 50,
    "total_tokens": 1050
  }
}
```

- `text` — 完整转录文本。

**客户端处理**：emit `subtitle-update {state:"final", text}` 并入库。关闭 SSE 流。

### 3. 错误事件（`error`）

```json
{
  "type": "error",
  "meta": {
    "session_id": "sse_xxx",
    "timestamp": 1642694400789
  },
  "message": "错误描述信息"
}
```

**客户端处理**：emit `asr-status {connected:true, last_error:Some(message)}`，不重试（一次性请求，无持久连接），继续处理下一个 utterance。

---

## 与 WebSocket 方案的差异

| 维度 | WS 流式（已废弃） | SSE 一次性（当前） |
| --- | --- | --- |
| 端点 | `wss://api.stepfun.com/v1/realtime/asr/stream` | `POST https://api.stepfun.com/step_plan/v1/audio/asr/sse` |
| 模型 | `stepaudio-2.5-asr-stream` | `stepaudio-2.5-asr` |
| 价格 | 1.2 元/小时 | 0.15 元/小时 |
| 连接 | 持久双向 | 单次请求-响应 |
| 音频提交 | 持续 40ms 帧推送 | 整段 utterance 一次提交 |
| 切句 | 服务端 VAD | 本地 silero-vad |
| 延迟 | 近实时 | 句尾 + SSE 往返（1-3s） |
| 副语言 | 返回 emotion/speech_rate | 不返回 |
| 重连 | 需指数退避 | 不需要（无持久连接） |

---

## 客户端实现要点

1. **请求构造**（`src/asr/protocol.rs`）：`AsrRequest` 结构序列化为上述 JSON，`audio.data` 为 Base64 编码的 PCM。
2. **流式解析**（`src/asr/client.rs`）：用 `reqwest::Response::bytes_stream()` 获取字节流，按 `\n\n` 分割事件块，每块解析 `data:` 行的 JSON 为 `SseEvent` 枚举。
3. **事件分发**（`src/asr/pipeline.rs` drainer 任务）：
   - `Delta` → 累积文本，emit `subtitle-update {state:"partial"}`
   - `Done` → emit `subtitle-update {state:"final"}`，`insert_transcription` 入库
   - `Error` → emit `asr-status`，继续下一个 utterance
4. **顺序保证**：utterance 通过 capacity=8 的 mpsc channel 串行提交给 drainer 任务，保证 final 顺序与说话顺序一致。
5. **背压**：channel 满时主任务阻塞，避免网络慢时 utterance 堆积无界。

---

## 备注

- 本协议基于阶跃星辰官方文档 `https://platform.stepfun.com/docs/zh/api-reference/audio/asr-sse`（2026-06-30 抓取）。
- Step Plan 端点与标准端点的差异仅在 URL 前缀，请求/响应格式相同。
- SSE 不支持 `full_rerun_on_commit`（二遍识别纠错），如需该能力需回退到 WebSocket 方案。
