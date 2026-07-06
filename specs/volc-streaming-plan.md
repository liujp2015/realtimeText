# 火山引擎 ASR 真正流式改造

## 目标
当前 volc 路线是"VAD 攒整句 → 整段提交 → 才开始收 partial"，导致用户说完一句才看到字（慢一步）。
改成：VAD 检测到**开口**就建连，边采集边上传音频块，partial 边收边显示，断句时发 last 收 final。

## 铁律（用户明确要求）
- **stepfun 路径一行不改**。`asr/client.rs` 的 `submit_utterance`（one-shot SSE POST 整段 base64）保持原样，行为完全不变。
- stepfun 本身不支持流式上传（API 决定），现状对它就是最优，不触碰。
- 只动 volc + pipeline + vad 事件层。

## 现状链路
```
run_main: DSP→VAD(512样本/32ms帧)→VadState::push_frame→Option<Utterance>(整段)→utt_tx
run_drainer: utt_rx→quantize→provider::submit_utterance(整段)→AsrEvent流→emit
```
- stepfun (`client.rs`): POST 整段 base64 → 流式收 SSE delta。**不能流式上传。**
- volc (`volc.rs`): 建连→发full→**连发所有音频段**→才开始收响应。批处理。

## 改造设计

### 1. VAD 事件化 — `src-tauri/src/vad/state.rs`
`push_frame` 返回值由 `Option<Utterance>` 改为 `Vec<VadEvent>`：
```rust
pub enum VadEvent {
    SpeechStart { start_ts: i64 },
    SpeechFrame { samples: Vec<f32> },     // 语音态下的每一帧(含尾部静音帧,与现状一致)
    SpeechEnd { start_ts: i64, end_ts: i64 }, // 断句边界(静音≥800ms 或 语音≥10s强制)
}
```
- **检测逻辑零改动**：SILENCE_DURATION_MS=800 / MAX_SPEECH_MS=10s / SPEECH_THRESHOLD=0.5 全部不变。
- 只改输出形态：Silence→Speech 转换发 `SpeechStart`+`SpeechFrame`；Speech 态每帧发 `SpeechFrame`；commit 时发 `SpeechFrame`+`SpeechEnd`。
- stepfun 在 drainer 里把 `SpeechFrame` 攒回 Vec，到 `SpeechEnd` 重建出与今天**逐字节相同**的 utterance（含尾部静音帧）→ 调原 `client::submit_utterance`。
- 更新 4 个单测断言新事件形态（语义不变）。

### 2. volc 流式接口 — `src-tauri/src/asr/volc.rs`
保留所有协议辅助函数（build_header / build_full_request / build_audio_request / parse_response / extract_text / gzip / 5 个单测）。
删除批处理入口 `submit_utterance_volc`，新增：
```rust
pub struct VolcStream { cmd_tx: Sender<VolcCmd>, events_rx: Receiver<AsrEvent> }
enum VolcCmd { Audio(Vec<u8>), Finish }
impl VolcStream {
    pub async fn start(api_key, resource_id, url, start_ts) -> Result<Self>;  // 建连+发full
    pub async fn send_audio(&mut self, pcm_s16le: &[u8]) -> Result<()>;        // 发一个音频段(seq递增)
    pub async fn finish(self, end_ts) -> Result<()>;                            // 发last包,等is_last
}
```
- `start` 内 `ws.split()` 成 sink+stream，spawn 一个 task 用 `select!` 并发：
  - 收 `VolcCmd::Audio` → `build_audio_request(seq, chunk, false)` → sink.send
  - 收 `VolcCmd::Finish` → 发 last 包（NEG_WITH_SEQ），停止接受 Audio，继续收响应到 is_last
  - 收 WS 响应 → `parse_response` → 有 text 发 `AsrEvent::Partial`，is_last 发 `Final`
- 这样 partial 在上传过程中就流式返回（真正实时）。

### 3. provider 分派 — `src-tauri/src/asr/provider.rs`
- 保留 `AsrConfig` 枚举（session.rs 仍构造）。
- 删除 `submit_utterance`（批处理入口不再需要）。
- stepfun drainer 直接调 `crate::asr::client::submit_utterance`（原函数，不动）。
- volc drainer 直接调 `crate::asr::volc::VolcStream`。

### 4. pipeline 事件驱动 — `src-tauri/src/asr/pipeline.rs`
- `run_main`：`VadState::push_frame` 返回 `Vec<VadEvent>`，逐个发到 `vad_tx`（替代原 `utt_tx`）。stop 信号处理不变。
- `run_drainer` 按 `AsrConfig` 分两条：
  - **Stepfun 分支**（等价现状）：SpeechStart→建 buffer；SpeechFrame→buffer.extend；SpeechEnd→quantize→`client::submit_utterance`→收 AsrEvent→emit。逐字节同今天。
  - **Volc 分支**：SpeechStart→`VolcStream::start` + spawn 事件转发任务(events_rx→emit)；SpeechFrame→攒到 ~100ms 量化后 `send_audio`；SpeechEnd→flush 余量→`finish`→等转发任务收完 Final。
- `UtteranceTask` 结构体移除，改用 `VadEvent` 通道。

### 5. 流式分块粒度
新增常量 `STREAM_CHUNK_MS = 100`（约 3 个 32ms 帧）。drainer 攒到 100ms 量化发一次，平衡延迟与包率（31→10 包/秒）。后续可调。

## 不改动的文件
- `asr/client.rs`（stepfun SSE）✅ 零改动
- `asr/protocol.rs`、`events.rs`、`commands/session.rs`、`config.rs`、前端 — 均不动
- `AsrEvent` 枚举、`SubtitleUpdate`、emit 逻辑 — 不动（partial 的 end_ts 本就传 None）

## 风险与缓解
| 风险 | 缓解 |
|---|---|
| stepfun 行为回退 | drainer 重建 utterance 含尾部静音帧，逐字节等价；client.rs 不碰 |
| VAD 事件化破坏断句 | 检测逻辑/常量零改，只改输出；4 单测更新断言 |
| volc 流式并发死锁 | select! 并发收发；Finish 后只收不发；drop 时 cmd_rx 返回 None 自然退出 |
| 长语音 10s 强制断 | 保留：SpeechEnd 触发 finish+final，下一句重新 SpeechStart 建连 |
| 中途 stop 中断流 | run_main 退出 drop vad_tx → drainer 退出 → VolcStream drop → task 退出 |

## 验证
1. `cargo test --lib` 全绿（含更新后的 4 个 VAD 单测 + 5 个 volc 单测）。
2. stepfun 回归：选 stepfun provider，说一句，确认字幕正常（行为同前）。
3. volc 实时：选 volc，**边说边看出字**（不再等说完）。日志看 `volc resp` 在 `SpeechFrame` 期间就有 partial。
4. 断句 final：说完停顿，确认 final 落库 + 显示。
