/// 运行时 ASR 配置：根据 provider 选择后端，参数随变体携带。
///
/// 注意：stepfun（one-shot SSE，整段提交）与 volc（流式 WS，边传边收）的提交模型不同，
/// 因此 pipeline 不再有统一批处理入口——drainer 按 AsrConfig 分两条路径各自处理：
/// - Stepfun：攒整段调 `asr::client::submit_utterance`
/// - Volc：开口即建 `asr::volc::VolcStream` 流式
#[derive(Debug, Clone)]
pub enum AsrConfig {
    Stepfun { api_key: String },
    Volc {
        api_key: String,
        resource_id: String,
        url: String,
    },
}
