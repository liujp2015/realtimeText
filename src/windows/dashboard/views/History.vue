<script setup lang="ts">
import { onMounted, ref } from "vue";
import {
  api,
  type SessionListItem,
  type TranscriptionRow,
  type SearchResultItem,
} from "@/lib/tauri";

const sessions = ref<SessionListItem[]>([]);
const total = ref(0);
const selectedGuid = ref<string | null>(null);
const detail = ref<TranscriptionRow[]>([]);
const keyword = ref("");
const results = ref<SearchResultItem[]>([]);

async function loadSessions() {
  const [t, items] = await api.sessionList(100, 0);
  total.value = t;
  sessions.value = items;
}

async function selectSession(guid: string) {
  selectedGuid.value = guid;
  const [, items] = await api.sessionGet(guid);
  detail.value = items;
}

async function deleteSession(guid: string) {
  if (!confirm("确认删除该会话及其全部字幕？")) return;
  await api.sessionDelete(guid);
  if (selectedGuid.value === guid) {
    selectedGuid.value = null;
    detail.value = [];
  }
  await loadSessions();
}

async function clearAll() {
  if (!confirm("确认清空全部历史？此操作不可撤销。")) return;
  await api.historyClear();
  selectedGuid.value = null;
  detail.value = [];
  results.value = [];
  await loadSessions();
}

async function search() {
  if (!keyword.value.trim()) {
    results.value = [];
    return;
  }
  results.value = await api.searchKeywords(keyword.value.trim());
}

function fmtTime(ts: number): string {
  return new Date(ts).toLocaleString();
}

function highlight(text: string, kw: string): string {
  if (!kw) return text;
  const idx = text.toLowerCase().indexOf(kw.toLowerCase());
  if (idx < 0) return text;
  return (
    text.slice(0, idx) +
    "<mark>" +
    text.slice(idx, idx + kw.length) +
    "</mark>" +
    text.slice(idx + kw.length)
  );
}

onMounted(loadSessions);
</script>

<template>
  <div class="history">
    <div class="left">
      <div class="toolbar">
        <h2>历史会话 ({{ total }})</h2>
        <button class="danger" @click="clearAll">清空全部</button>
      </div>
      <ul class="session-list">
        <li
          v-for="s in sessions"
          :key="s.guid"
          :class="{ active: selectedGuid === s.guid }"
          @click="selectSession(s.guid)"
        >
          <div class="title">{{ fmtTime(s.started_at * 1000) }}</div>
          <div class="meta">
            {{ s.device_name }} · {{ s.transcription_count }} 条
            <span v-if="s.ended_at">· 已结束</span>
          </div>
          <button class="del" @click.stop="deleteSession(s.guid)">删除</button>
        </li>
      </ul>
    </div>

    <div class="right">
      <div class="search">
        <input
          v-model="keyword"
          placeholder="跨会话关键词检索..."
          @input="search"
        />
        <span v-if="results.length">{{ results.length }} 条命中</span>
      </div>
      <div v-if="results.length" class="results">
        <div v-for="r in results" :key="r.id" class="result-item" @click="selectSession(r.session_guid)">
          <div class="result-text" v-html="highlight(r.text, keyword)"></div>
          <div class="result-meta">{{ fmtTime(r.start_ts) }}</div>
        </div>
      </div>
      <div v-else-if="selectedGuid" class="detail">
        <h3>会话详情</h3>
        <div v-for="d in detail" :key="d.id" class="transcription">
          <div class="text">{{ d.text }}</div>
          <div class="meta">
            {{ fmtTime(d.start_ts) }} – {{ fmtTime(d.end_ts) }}
            <span v-if="d.paralinguistic" class="tag">{{ d.paralinguistic }}</span>
          </div>
        </div>
      </div>
      <div v-else class="empty">选择左侧会话查看详情，或使用搜索框检索。</div>
    </div>
  </div>
</template>

<style scoped>
.history { display: flex; gap: 16px; height: 100%; }
.left { width: 320px; background: #fff; border-radius: 6px; padding: 12px; overflow: auto; }
.right { flex: 1; background: #fff; border-radius: 6px; padding: 12px; overflow: auto; }
.toolbar { display: flex; justify-content: space-between; align-items: center; }
.toolbar h2 { margin: 0; font-size: 14px; }
.session-list { list-style: none; padding: 0; margin: 12px 0 0; }
.session-list li {
  padding: 8px 10px;
  border-radius: 4px;
  cursor: pointer;
  position: relative;
}
.session-list li:hover { background: #f0f0f0; }
.session-list li.active { background: #e0f2fe; }
.title { font-weight: 500; font-size: 13px; }
.meta { font-size: 12px; color: #666; margin-top: 2px; }
.del {
  position: absolute;
  right: 8px;
  top: 8px;
  font-size: 11px;
  padding: 2px 8px;
  background: #ef4444;
  color: #fff;
  border: 0;
  border-radius: 3px;
  cursor: pointer;
  opacity: 0;
}
.session-list li:hover .del { opacity: 1; }
.search { display: flex; gap: 8px; align-items: center; }
.search input {
  flex: 1;
  padding: 6px 10px;
  border: 1px solid #ccc;
  border-radius: 4px;
}
.results { margin-top: 12px; }
.result-item {
  padding: 8px;
  border-bottom: 1px solid #eee;
  cursor: pointer;
}
.result-item:hover { background: #f9f9f9; }
.result-text { font-size: 14px; }
.result-meta { font-size: 12px; color: #666; margin-top: 2px; }
mark { background: #fef08a; padding: 0 2px; }
.detail h3 { margin: 0 0 12px; font-size: 14px; }
.transcription {
  padding: 8px 0;
  border-bottom: 1px solid #eee;
}
.transcription .text { font-size: 14px; }
.transcription .meta { font-size: 12px; color: #666; margin-top: 4px; }
.tag {
  background: #e0e7ff;
  padding: 1px 6px;
  border-radius: 3px;
  margin-left: 8px;
}
.empty { color: #999; padding: 40px 0; text-align: center; }
.danger { background: #ef4444; color: #fff; border: 0; padding: 4px 10px; border-radius: 4px; cursor: pointer; }
</style>
