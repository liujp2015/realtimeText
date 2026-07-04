# Data Model: Windows 实时字幕工具

**Date**: 2026-06-30
**Last Updated**: 2026-06-30（适配 SSE 方案，副语言字段废弃）
**Source**: spec.md §Key Entities + research.md R7/R10

## 实体关系

```text
Session 1 ──── * Transcription
AppConfig (单行 key-value，全局）
```

## 表结构（DDL）

### `sessions` — 会话

| 列名 | 类型 | 约束 | 说明 |
| --- | --- | --- | --- |
| `guid` | TEXT | PRIMARY KEY | 会话唯一标识（UUID v4） |
| `started_at` | BIGINT | NOT NULL | 会话开始 Unix 时间戳（秒） |
| `ended_at` | BIGINT | NULL | 会话结束时间戳，运行中为 NULL |
| `device_name` | TEXT | NOT NULL | 捕获时的输出设备名（便于回看上下文） |

索引：无额外索引（主键即 guid）。

### `transcriptions` — 字幕条

| 列名 | 类型 | 约束 | 说明 |
| --- | --- | --- | --- |
| `id` | INTEGER | PRIMARY KEY AUTOINCREMENT | 自增主键 |
| `session_guid` | TEXT | NOT NULL, FK → sessions.guid ON DELETE CASCADE | 所属会话 |
| `text` | TEXT | NOT NULL | 最终识别文本 |
| `start_ts` | BIGINT | NOT NULL | 句子起始 Unix 时间戳（毫秒） |
| `end_ts` | BIGINT | NOT NULL | 句子结束 Unix 时间戳（毫秒） |
| `paralinguistic` | TEXT | NULL | 副语言 JSON 字符串（见下） |
| `created_at` | BIGINT | NOT NULL DEFAULT (strftime('%s','now')) | 入库时间 |

索引：
- `idx_transcriptions_session_guid` ON (`session_guid`)
- `idx_transcriptions_created_at` ON (`created_at`)
- `idx_transcriptions_text` ON (`text`) — 用于关键词 LIKE 检索的最低保障

### `app_config` — 全局配置（key-value）

| 列名 | 类型 | 约束 | 说明 |
| --- | --- | --- | --- |
| `key` | TEXT | PRIMARY KEY | 配置键 |
| `value` | TEXT | NOT NULL | 配置值（JSON 字符串） |

已知键：
- `api_key` — 阶跃星辰 API Key（明文）
- `appearance` — `{ font_family, font_size, text_color, bg_opacity }`
- `window` — `{ x, y, width, height }`

## 副语言元数据结构（`transcriptions.paralinguistic` JSON）

> **⚠️ 已废弃（2026-06-30）**：SSE 一次性接口（`stepaudio-2.5-asr`）不返回副语言信息。该字段在当前实现中永远为 NULL，保留列结构是为了向前兼容旧数据与未来回退到 WebSocket 方案。

```json
{
  "emotion": "neutral|happy|sad|angry|frustrated|surprised",
  "speech_rate": "slow|normal|fast",
  "non_verbal": ["laugh", "sigh", "cough"]
}
```

所有字段均可为空；模型未下发副语言时整列存 NULL。当前 SSE 方案下整列恒为 NULL。

## 状态转换

### Session 状态机

```text
[created] ──start──> [active] ──stop──> [ended]
```

- `created`：仅持久化记录已建立，音频/VAD/ASR 管道未启动（瞬态，通常立即进入 active）
- `active`：`ended_at IS NULL`，音频捕获、VAD 切句、SSE 提交运行中
- `ended`：`ended_at IS NOT NULL`，资源已释放（pipeline 任务 5s 超时 await 后再 finalize）

### Transcription 状态

字幕条只持久化稳态（final）结果；partial 结果仅在前端内存中流转，不入库。这是设计决策，避免高频写入。

**当前 SSE 方案下**：final 来自 SSE `transcript.text.done` 事件，每个 utterance 对应一条 transcription 记录。一个 utterance 通常是一句话（VAD 静音 800ms 切句）或最长 10s 的连续语音（强制提交）。

## 验证规则

- `sessions.guid` 必须为合法 UUID v4
- `transcriptions.start_ts < end_ts`
- `transcriptions.text` 不为空字符串
- `app_config.value` 必须为合法 JSON
- `appearance.font_size` 取值范围 [12, 72]
- `appearance.bg_opacity` 取值范围 [0.0, 1.0]
- `api_key` 不为空字符串（启动识别时校验，违反时 FR-013 提示）

## 检索语义

关键词检索（FR-009）：

```sql
SELECT t.*, s.started_at AS session_started_at
FROM transcriptions t
JOIN sessions s ON t.session_guid = s.guid
WHERE t.text LIKE '%' || :keyword || '%'
ORDER BY t.start_ts DESC
LIMIT 200;
```

- 大小写敏感（SQLite 默认 LIKE 对 ASCII 区分大小写；中文无大小写问题）
- 命中数上限 200，避免超大结果集拖慢 UI
- 前端对命中关键词做高亮渲染

## 迁移文件

`src-tauri/migrations/0001_init.sql` 包含上述三表 DDL 与索引。通过 `sqlx::migrate!("./migrations")` 在应用首次启动时执行。

## 数据生命周期

- **创建**：稳态字幕在 SSE 收到 `transcript.text.done` 事件时由 `db::repository::insert_transcription` 写入（pipeline drainer 任务调用）
- **读取**：历史面板按 `sessions.started_at DESC` 分页拉取；会话详情按 `transcriptions.start_ts ASC` 拉取
- **删除**：删除 session 级联删除其下所有 transcription（`ON DELETE CASCADE`）
- **保留**：无自动过期策略（用户本地数据，由用户手动删除，FR-010）
