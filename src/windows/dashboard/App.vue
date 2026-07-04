<script setup lang="ts">
import { onMounted, ref } from "vue";
import { RouterLink, RouterView } from "vue-router";
import { useSettingsStore } from "@/stores/settings";
import { useAsrStatus } from "@/composables/useAsrStatus";
import { api } from "@/lib/tauri";

const settings = useSettingsStore();
const { connected, retryCount, lastError } = useAsrStatus();
const running = ref(false);

async function toggleSession() {
  try {
    if (running.value) {
      await api.sessionStop();
      running.value = false;
    } else {
      await api.sessionStart();
      running.value = true;
    }
  } catch (e) {
    alert(String(e));
  }
}

onMounted(async () => {
  await settings.load();
});
</script>

<template>
  <div class="dashboard">
    <header class="topbar">
      <h1>实时字幕工具</h1>
      <nav>
        <RouterLink to="/settings">设置</RouterLink>
        <RouterLink to="/history">历史</RouterLink>
      </nav>
      <div class="status">
        <span class="dot" :class="{ on: connected, off: !connected }"></span>
        <span v-if="connected">已连接</span>
        <span v-else>未连接{{ retryCount > 0 ? `（重试 ${retryCount}）` : "" }}</span>
        <span v-if="lastError" class="err">{{ lastError }}</span>
      </div>
      <button class="toggle" :class="{ active: running }" @click="toggleSession">
        {{ running ? "停止字幕" : "开始字幕" }}
      </button>
    </header>
    <main>
      <RouterView />
    </main>
  </div>
</template>

<style scoped>
.dashboard {
  font-family: "Microsoft YaHei", sans-serif;
  height: 100vh;
  display: flex;
  flex-direction: column;
}
.topbar {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 12px 20px;
  background: #1e1e1e;
  color: #fff;
}
.topbar h1 { font-size: 16px; margin: 0; }
.topbar nav a {
  color: #aaa;
  text-decoration: none;
  padding: 4px 10px;
  border-radius: 4px;
}
.topbar nav a.router-link-active {
  background: #333;
  color: #fff;
}
.status {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
}
.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}
.dot.on { background: #4ade80; }
.dot.off { background: #f87171; }
.err { color: #f87171; }
.toggle {
  padding: 6px 16px;
  border: 0;
  border-radius: 4px;
  background: #3b82f6;
  color: #fff;
  cursor: pointer;
}
.toggle.active { background: #ef4444; }
main {
  flex: 1;
  overflow: auto;
  padding: 20px;
  background: #f5f5f5;
}
</style>
