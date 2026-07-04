import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ---- IPC event payload types (mirror contracts/ipc-events.md) ----

export type SubtitleUpdate = {
  state: "partial" | "final";
  text: string;
  start_ts: number;
  end_ts?: number;
  paralinguistic?: {
    emotion?: string;
    speech_rate?: string;
    non_verbal?: string[];
  };
};

export type SessionMeta = {
  session_guid: string;
  started_at: number;
  device_name: string;
};

export type AsrStatus = {
  connected: boolean;
  retry_count: number;
  last_error?: string;
};

// ---- Command types (mirror contracts/commands.md) ----

export type SessionStartInfo = {
  session_guid: string;
  started_at: number;
  device_name: string;
};

export type SessionListItem = {
  guid: string;
  started_at: number;
  ended_at: number | null;
  device_name: string;
  transcription_count: number;
};

export type TranscriptionRow = {
  id: number;
  session_guid: string;
  text: string;
  start_ts: number;
  end_ts: number;
  paralinguistic: string | null;
  created_at: number;
};

export type SearchResultItem = {
  id: number;
  session_guid: string;
  session_started_at: number;
  text: string;
  start_ts: number;
  end_ts: number;
};

export type Appearance = {
  font_family: string;
  font_size: number;
  text_color: string;
  bg_opacity: number;
};

// ---- Typed wrappers ----

export const api = {
  sessionStart: () => invoke<SessionStartInfo>("session_start"),
  sessionStop: () => invoke<void>("session_stop"),
  sessionList: (limit = 50, offset = 0) =>
    invoke<[number, SessionListItem[]]>("session_list", { limit, offset }),
  sessionGet: (guid: string) =>
    invoke<[SessionListItem | null, TranscriptionRow[]]>("session_get", { guid }),
  sessionDelete: (guid: string) => invoke<void>("session_delete", { guid }),
  historyClear: () => invoke<void>("history_clear"),
  searchKeywords: (keyword: string) =>
    invoke<SearchResultItem[]>("search_keywords", { keyword }),
  configGet: (key: string) => invoke<unknown | null>("config_get", { key }),
  configSet: (key: string, value: unknown) =>
    invoke<void>("config_set", { key, value }),
  configResetAppearance: () => invoke<Appearance>("config_reset_appearance"),
};

export function onSubtitleUpdate(
  handler: (payload: SubtitleUpdate) => void
): Promise<UnlistenFn> {
  return listen<SubtitleUpdate>("subtitle-update", (e) => handler(e.payload));
}

export function onSessionMeta(
  handler: (payload: SessionMeta) => void
): Promise<UnlistenFn> {
  return listen<SessionMeta>("session-meta", (e) => handler(e.payload));
}

export function onAsrStatus(
  handler: (payload: AsrStatus) => void
): Promise<UnlistenFn> {
  return listen<AsrStatus>("asr-status", (e) => handler(e.payload));
}
