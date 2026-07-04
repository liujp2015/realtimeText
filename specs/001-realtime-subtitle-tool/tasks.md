---

description: "Task list for Windows 实时字幕工具"
---

# Tasks: Windows 实时字幕工具

**Input**: Design documents from `/specs/001-realtime-subtitle-tool/`

**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/

**Tests**: 未在 spec 中显式要求 TDD；测试任务以 `cargo test` 单元/集成为主，标记为可选 [P]。

**Organization**: 按 spec.md 的 3 个用户故事分组（US1 实时字幕覆盖 / US2 历史检索 / US3 样式定制）。

## Format: `[ID] [P?] [Story] Description`

- **[P]**: 可并行（不同文件、无依赖）
- **[Story]**: 所属用户故事（US1/US2/US3）
- 任务描述包含确切文件路径

## Path Conventions

- Tauri v2 双目录：`src-tauri/`（Rust 后端）+ `src/`（前端 Vue 3）
- Rust 测试：`src-tauri/tests/`
- 前端测试：`src/**/*.test.ts`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Tauri v2 项目骨架与工具链初始化

- [x] T001 Initialize Tauri v2 project with Vue 3 + TypeScript frontend via `npm create tauri-app@latest` (produces `src-tauri/`, `src/`, `package.json`, `tauri.conf.json`)
- [x] T002 Configure `src-tauri/Cargo.toml` with dependencies: tauri v2, cpal, rubato, rtrb, tokio, tokio-tungstenite, sqlx (sqlite), serde, serde_json, base64, tauri-plugin-log, uuid, anyhow, thiserror
- [x] T003 [P] Configure `package.json` with frontend deps: vue 3, vite, @tauri-apps/api v2, @tauri-apps/plugin-log, pinia, vue-router
- [x] T004 [P] Configure `src-tauri/tauri.conf.json` windows: `transparent: true`, `decorations: false`, `alwaysOnTop: true`, `skipTaskbar: true`, `resizable: true`; define two windows (`subtitle` + `dashboard`) with entry HTMLs
- [x] T005 [P] Setup ESLint + Prettier + rustfmt + clippy configs at repo root
- [x] T006 [P] Configure Vite multi-page entry in `vite.config.ts` for `subtitle.html` and `dashboard.html`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: 所有用户故事共享的核心基础设施

**⚠️ CRITICAL**: 本阶段未完成前不得开始任何用户故事

- [x] T007 Define `AppState` struct in `src-tauri/src/state.rs` (fields: `SqlitePool`, `AppConfig`, `Option<RunningHandle>`, `AppHandle`)
- [x] T008 [P] Create `src-tauri/migrations/0001_init.sql` with DDL for `sessions`, `transcriptions`, `app_config` tables and indexes per `data-model.md`
- [x] T009 [P] Implement `src-tauri/src/db/pool.rs`: initialize `SqlitePool` via `sqlx::sqlite::SqlitePoolOptions`, run `sqlx::migrate!("./migrations")`, resolve DB path via `app_handle.path().app_data_dir()`
- [x] T010 [P] Implement `src-tauri/src/audio/ring.rs`: wrap `rtrb::Producer` + `rtrb::Consumer` for f32 samples, capacity sized for ~1s of 48kHz stereo (≈ 384000 samples)
- [x] T011 [P] Implement `src-tauri/src/audio/dsp.rs`: stereo downmix, rubato 48k→16k resampler, hard clip, f32→i16 quantization, 40ms/1280B frame accumulation; pure functions for testability
- [x] T012 [P] Implement `src-tauri/src/asr/protocol.rs`: serde structs for `session.update`, `input_audio_buffer.append`, partial/final/error events per `contracts/ws-protocol.md`
- [x] T013 [P] Implement `src-tauri/src/events.rs`: typed `emit_subtitle_update`, `emit_session_meta`, `emit_asr_status` helpers wrapping `AppHandle::emit`
- [x] T014 [P] Implement `src-tauri/src/logging.rs`: configure `tauri-plugin-log` with `TargetKind::LogDir`, `max_file_size(50_000)`, `TimezoneStrategy::UseLocal`, `forwardConsole` enabled
- [x] T015 [P] Implement `src-tauri/src/db/repository.rs`: `insert_session`, `finalize_session`, `insert_transcription`, `list_sessions`, `get_session_with_transcriptions`, `delete_session`, `clear_history`, `search_keywords`, `get_config`, `set_config` per `data-model.md` and `contracts/commands.md`
- [x] T016 [P] Implement `src-tauri/src/commands/config.rs`: `#[tauri::command] config_get` / `config_set` / `config_reset_appearance` with validation rules from `data-model.md`
- [x] T017 [P] Implement `src-tauri/src/commands/session.rs`: `#[tauri::command] session_list` / `session_get` / `session_delete` / `history_clear` (read-only or simple destructive)
- [x] T018 [P] Implement `src-tauri/src/commands/search.rs`: `#[tauri::command] search_keywords` returning ≤200 results ordered by `start_ts DESC`
- [x] T019 Wire up `src-tauri/src/main.rs`: register plugin-log, invoke `db::pool::init`, register all commands, manage `AppState`, create both windows
- [x] T020 [P] Create frontend shared lib `src/lib/tauri.ts`: typed wrappers for `invoke` and `listen` with TS types mirroring `contracts/`
- [x] T021 [P] Create frontend Pinia store `src/stores/settings.ts`: API key, appearance, window position state with persistence via `config_get`/`config_set`
- [x] T022 [P] Create frontend composable `src/composables/useAsrStatus.ts`: subscribe `asr-status` event, expose reactive `connected` / `retryCount` / `lastError`

**Checkpoint**: 基础设施就绪——DB 可读写、配置可存取、日志可写、命令可调用、事件可发。可开始用户故事实现。

---

## Phase 3: User Story 1 - 观看视频时获得实时字幕覆盖 (Priority: P1) 🎯 MVP

**Goal**: 用户在 Windows 上播放任意播放器内容时，屏幕上方出现悬浮透明置顶字幕窗，实时滚动显示识别文字，鼠标穿透，句尾归档。

**Independent Test**: 启动工具 → 配置 API Key → 开启字幕 → 播放器播放带人声音视频 → 2 秒内字幕滚动出现 → 句尾停顿后归档 → 鼠标点击穿透到播放器。

### Implementation for User Story 1

- [x] T023 [US1] Implement `src-tauri/src/audio/capture.rs`: open default output device via cpal, build loopback stream at 48kHz/stereo/f32, in callback push raw bytes into `rtrb::Producer` (no allocation, no locks)
- [x] T024 [US1] Implement `src-tauri/src/audio/dsp.rs` worker loop (extends T011): spawn Tokio task that pulls from `rtrb::Consumer`, runs DSP chain, accumulates 1280-byte frames, exposes via channel
- [x] T025 [US1] Implement `src-tauri/src/asr/client.rs`: `AsrClient` with `connect(api_key)`, sends `session.update` on open, exposes `send_audio_frame(base64)`, handles partial/final/error messages, surfaces paralinguistic payload
- [x] T026 [US1] Implement `src-tauri/src/asr/pipeline.rs`: orchestrator that owns `AsrClient` + DSP frame channel + reconnect loop (exponential backoff 1/2/4/8/16/30s per `contracts/ws-protocol.md`), buffers up to 30s of frames during disconnect, replays on reconnect
- [x] T027 [US1] Implement `src-tauri/src/commands/session.rs` `session_start` / `session_stop` (extends T017): validate API key (FR-013), create session row, spawn capture + pipeline, emit `session-meta`, return session info; on stop: close WS, stop stream, write `ended_at`
- [x] T028 [US1] Add device-change monitor in `src-tauri/src/audio/capture.rs`: 2s polling of `default_output_device()`, on change tear down stream + rebuild (≤3s SLA per SC-008), emit `asr-status` advisory
- [x] T029 [US1] Wire `subtitle-update` emit in `src-tauri/src/asr/client.rs`: on partial → `emit_subtitle_update(state="partial")`, on final → `emit_subtitle_update(state="final")` + `repository::insert_transcription`
- [ ] T030 [US1] Implement DRM detection hint in `src-tauri/src/audio/capture.rs`: if stream consistently yields zero/silence samples for >10s while user expects audio, emit `asr-status` with `last_error="可能受 DRM 保护"` (best-effort, non-blocking)
- [x] T031 [US1] Create `src/windows/subtitle/App.vue`: Pinia-connected subtitle overlay; current draft line (partial) + last N final lines; auto-scroll; reads appearance from store
- [x] T032 [US1] Create `src/windows/subtitle/style.css`: default appearance (font-size 24, bg-opacity 0.5, white text, dark translucent backdrop)
- [x] T033 [US1] Create `src/composables/useSubtitle.ts`: subscribe `subtitle-update`, expose reactive `draft` and `finals` arrays
- [x] T034 [US1] Implement window drag in `src/windows/subtitle/App.vue`: temporary `set_ignore_cursor_events(false)` on drag handle, restore `true` on release; default state is `ignore_cursor_events(true)` per FR-003
- [x] T035 [US1] Create `src/windows/dashboard/App.vue` shell with router: Settings view + History view + Start/Stop subtitle button + connection status indicator (binds `useAsrStatus`)
- [x] T036 [US1] Implement Settings view `src/windows/dashboard/views/Settings.vue`: API Key input (masked), save via `config_set`; if missing/invalid on `session_start`, surface error per FR-013
- [x] T037 [US1] Register `subtitle-update` / `session-meta` / `asr-status` listeners with proper unlisten in `onBeforeUnmount` for all relevant composables

**Checkpoint**: US1 完整可用——MVP 达成。可独立部署演示。

---

## Phase 4: User Story 2 - 检索与回看历史字幕 (Priority: P2)

**Goal**: 用户可在历史面板查看会话列表、会话详情，按关键词跨会话检索，删除会话或清空历史。

**Independent Test**: 完成 ≥30s 识别 → 停止 → 进入历史面板 → 看到会话 → 查看详情 → 输入关键词检索命中 → 删除会话验证级联。

### Implementation for User Story 2

- [x] T038 [P] [US2] Implement History view `src/windows/dashboard/views/History.vue`: paginated session list (uses `session_list`), click → detail panel
- [x] T039 [P] [US2] Implement Session Detail component `src/windows/dashboard/components/SessionDetail.vue`: chronological transcription list with timestamps + paralinguistic tags (uses `session_get`)
- [x] T040 [US2] Implement Search bar `src/windows/dashboard/components/SearchBar.vue`: input → `search_keywords` → result list with keyword highlight, session link, timestamp (uses `search_keywords` per `contracts/commands.md`)
- [x] T041 [US2] Implement delete actions in `History.vue`: per-session delete (`session_delete`) and "clear all" (`history_clear`) with confirmation dialog
- [ ] T042 [US2] Add LIKE-based search test fixture in `src-tauri/tests/db/repository_test.rs` (optional [P]): verify `search_keywords` returns expected rows, honors 200-result limit
- [x] T043 [US2] Add virtualized scrolling to `SessionDetail.vue` and search results to maintain fluency under large datasets (per SC-004 "scrolling stays smooth")

**Checkpoint**: US1 + US2 均可独立工作。字幕即时显示 + 历史检索完整闭环。

---

## Phase 5: User Story 3 - 调整字幕外观与位置 (Priority: P3)

**Goal**: 用户可调整字幕字体/字号/颜色/底板透明度，拖动字幕窗到任意位置，配置在重启后保留，可重置为默认。

**Independent Test**: 改字号与透明度 → 实时生效 → 拖动字幕窗 → 关闭工具 → 重启 → 样式与位置恢复 → 重置为默认。

### Implementation for User Story 3

- [x] T044 [P] [US3] Extend Settings view with Appearance panel `src/windows/dashboard/views/Settings.vue` (or separate `Appearance.vue`): font family dropdown, font size slider [12,72], text color picker, bg opacity slider [0,1], live preview
- [x] T045 [US3] Wire appearance changes in `src/stores/settings.ts`: on change → `config_set({key:"appearance", value})` → broadcast to subtitle window via new `appearance-changed` event or shared Pinia
- [x] T046 [US3] Make `src/windows/subtitle/App.vue` reactive to appearance store: CSS variables bound to font-family/size/color/opacity, instant update without remount
- [x] T047 [US3] Implement window position persistence in `src/windows/subtitle/App.vue`: on `tauri.window.onMoved` → `config_set({key:"window", value:{x,y,w,h}})`; on mount → restore position via `window.setPosition`
- [x] T048 [US3] Implement "Reset to default" button in Appearance panel → `config_reset_appearance` → update store + subtitle window
- [ ] T049 [US3] Constrain window drag within visible screen bounds in `src/windows/subtitle/App.vue` (T034 enhancement): clamp x/y to current monitor work area

**Checkpoint**: 全部 3 个用户故事独立可测、可演示。

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: 跨故事打磨与发布准备

- [ ] T050 [P] Add `cargo test` unit tests for DSP in `src-tauri/tests/audio/dsp_test.rs`: downmix correctness, resampler output sample rate, quantization clamping, 1280-byte frame boundary
- [ ] T051 [P] Add `cargo test` unit tests for ring buffer in `src-tauri/tests/audio/ring_test.rs`: producer/consumer under load, no overrun
- [x] T052 [P] Add `cargo test` unit tests for WS protocol serialization in `src-tauri/tests/asr/protocol_test.rs`: round-trip `session.update` / `input_audio_buffer.append` / partial / final
- [ ] T053 [P] Add `cargo test` integration tests for DB repository in `src-tauri/tests/db/repository_test.rs`: CRUD, cascade delete, search, config validation rules
- [ ] T054 [P] Add frontend Vitest tests for composables: `useSubtitle`, `useAsrStatus`, `settings` store
- [ ] T055 Audit memory leaks: verify event unlisten called everywhere, no orphaned Tokio tasks after `session_stop`, ring buffer drops on shutdown
- [ ] T056 [P] Run 30-min soak test per quickstart.md Scene 2: confirm memory growth ≤30%, no crash
- [ ] T057 [P] Run 8-hour log size test per quickstart.md Scene 9: confirm ≤50MB total
- [x] T058 [P] Add user-facing README at repo root: install, API Key setup, DRM limitation note, multi-player mixing caveat
- [x] T059 [P] Configure Windows MSI/NSIS bundler in `tauri.conf.json` `bundle` section
- [ ] T060 Run all 10 quickstart.md scenarios end-to-end and record results in validation table

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately
- **Foundational (Phase 2)**: Depends on Phase 1 — BLOCKS all user stories
- **US1 (Phase 3)**: Depends on Phase 2 — MVP path
- **US2 (Phase 4)**: Depends on Phase 2; integrates with US1 outputs (transcription rows) but can be developed in parallel
- **US3 (Phase 5)**: Depends on Phase 2; purely UI/store work, fully parallel with US1/US2
- **Polish (Phase 6)**: After all desired stories complete

### User Story Dependencies

- **US1 (P1)**: 独立。MVP 范围 = Phase 1 + 2 + 3
- **US2 (P2)**: 依赖 US1 已产生稳态字幕数据（运行时依赖），代码层面可并行开发
- **US3 (P3)**: 完全独立，纯前端样式/位置工作

### Within Each User Story

- Models/repos before services
- Rust backend before frontend wiring
- Wire events/listeners last with proper lifecycle

### Parallel Opportunities

- Phase 1: T003–T006 all [P]
- Phase 2: T008–T018 大多 [P]（不同文件）
- Phase 3 (US1): T031–T036 前端与 T023–T030 后端可并行（不同人/不同时）
- Phase 4 (US2): T038/T039 [P] 并行
- Phase 5 (US3): T044 与 T050–T054 [P] 并行
- Phase 6: T050–T054 测试任务全部 [P]

---

## Parallel Example: User Story 1

```bash
# Backend tasks (one developer):
Task T023: capture.rs
Task T024: dsp worker loop
Task T025: asr/client.rs
Task T026: asr/pipeline.rs

# Frontend tasks (parallel, another developer):
Task T031: windows/subtitle/App.vue
Task T033: composables/useSubtitle.ts
Task T035: windows/dashboard/App.vue
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL)
3. Complete Phase 3: US1
4. **STOP and VALIDATE**: Run quickstart.md Scenes 1, 2, 3, 5, 6, 8, 10
5. Deploy/demo if ready

### Incremental Delivery

1. Setup + Foundational → 基础设施就绪
2. + US1 → MVP（实时字幕覆盖）
3. + US2 → 历史检索闭环
4. + US3 → 个性化定制
5. Polish → 发布候选

### Parallel Team Strategy

- 后端 1 人：Phase 2 Rust 基础 → US1 后端 → US2 命令
- 前端 1 人：Phase 2 共享 lib/store → US1 前端 → US3
- 测试 1 人：Phase 6 测试套件 + quickstart 验证

---

## Notes

- [P] = 不同文件、无依赖
- [Story] 标签映射到 spec.md 用户故事
- 每个故事独立可测可演示
- 在 checkpoint 处提交并验证
- 避免：模糊任务、同文件冲突、跨故事硬依赖
