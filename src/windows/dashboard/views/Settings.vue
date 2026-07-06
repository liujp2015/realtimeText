<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useSettingsStore } from "@/stores/settings";
import type { Appearance } from "@/lib/tauri";
import {
  DEFAULT_VOLC_RESOURCE_ID,
  DEFAULT_VOLC_URL,
  type AsrProvider,
} from "@/stores/settings";

const settings = useSettingsStore();
const apiKeyInput = ref("");
const provider = ref<AsrProvider>("stepfun");
const volcApiKeyInput = ref("");
const volcResourceIdInput = ref(DEFAULT_VOLC_RESOURCE_ID);
const volcUrlInput = ref(DEFAULT_VOLC_URL);
const showAdvanced = ref(false);
const appearance = ref<Appearance>({
  font_family: "Microsoft YaHei",
  font_size: 24,
  text_color: "#FFFFFF",
  bg_opacity: 0.5,
});
const saved = ref(false);

onMounted(async () => {
  await settings.load();
  apiKeyInput.value = settings.apiKey;
  provider.value = settings.provider;
  volcApiKeyInput.value = settings.volcApiKey;
  volcResourceIdInput.value = settings.volcResourceId;
  volcUrlInput.value = settings.volcUrl;
  appearance.value = { ...settings.appearance };
});

async function saveApiKey() {
  await settings.saveApiKey(apiKeyInput.value.trim());
  flashSaved();
}

async function saveProvider() {
  await settings.saveProvider(provider.value);
  flashSaved();
}

async function saveVolcConfig() {
  await settings.saveVolcApiKey(volcApiKeyInput.value.trim());
  await settings.saveVolcResourceId(
    volcResourceIdInput.value.trim() || DEFAULT_VOLC_RESOURCE_ID
  );
  await settings.saveVolcUrl(volcUrlInput.value.trim() || DEFAULT_VOLC_URL);
  flashSaved();
}

async function saveAppearance() {
  await settings.saveAppearance({ ...appearance.value });
  flashSaved();
}

async function resetAppearance() {
  await settings.resetAppearance();
  appearance.value = { ...settings.appearance };
  flashSaved();
}

function flashSaved() {
  saved.value = true;
  setTimeout(() => (saved.value = false), 1500);
}
</script>

<template>
  <div class="settings">
    <section>
      <h2>ASR 服务</h2>
      <p class="hint">选择实时语音转写引擎，下方按选择展示对应配置。</p>
      <div class="row">
        <select v-model="provider" @change="saveProvider">
          <option value="stepfun">阶跃星辰 StepAudio</option>
          <option value="volc">火山引擎 SAUC</option>
        </select>
      </div>
    </section>

    <section v-if="provider === 'stepfun'">
      <h2>阶跃星辰 API Key</h2>
      <p class="hint">StepAudio 实时识别服务密钥。</p>
      <div class="row">
        <input
          v-model="apiKeyInput"
          type="password"
          placeholder="sk-..."
          class="input"
        />
        <button @click="saveApiKey">保存</button>
      </div>
    </section>

    <section v-else>
      <h2>火山引擎配置</h2>
      <p class="hint">SAUC 流式 ASR 服务密钥与模型资源。</p>
      <label class="field">
        <span>API Key</span>
        <input
          v-model="volcApiKeyInput"
          type="password"
          placeholder="火山引擎 API Key"
          class="input"
        />
      </label>
      <label class="field">
        <span>模型 / Resource ID</span>
        <input v-model="volcResourceIdInput" class="input" />
      </label>
      <div class="row">
        <button @click="saveVolcConfig">保存</button>
        <button class="secondary" @click="showAdvanced = !showAdvanced">
          {{ showAdvanced ? "收起高级" : "高级" }}
        </button>
        <span v-if="saved" class="saved">已保存</span>
      </div>
      <label v-if="showAdvanced" class="field">
        <span>WebSocket URL</span>
        <input v-model="volcUrlInput" class="input" />
      </label>
    </section>

    <section>
      <h2>字幕样式</h2>
      <div class="grid">
        <label>字体
          <select v-model="appearance.font_family">
            <option>Microsoft YaHei</option>
            <option>SimHei</option>
            <option>SimSun</option>
            <option>Arial</option>
            <option>Segoe UI</option>
          </select>
        </label>
        <label>字号 (12–72)
          <input
            v-model.number="appearance.font_size"
            type="number"
            min="12"
            max="72"
          />
        </label>
        <label>文字颜色
          <input v-model="appearance.text_color" type="color" />
        </label>
        <label>底板透明度 (0–1)
          <input
            v-model.number="appearance.bg_opacity"
            type="range"
            min="0"
            max="1"
            step="0.05"
          />
          <span>{{ appearance.bg_opacity.toFixed(2) }}</span>
        </label>
      </div>
      <div class="row">
        <button @click="saveAppearance">应用</button>
        <button class="secondary" @click="resetAppearance">重置为默认</button>
      </div>
    </section>
  </div>
</template>

<style scoped>
.settings { max-width: 720px; }
section {
  background: #fff;
  padding: 20px;
  margin-bottom: 16px;
  border-radius: 6px;
}
h2 { margin: 0 0 8px; font-size: 16px; }
.hint { color: #666; font-size: 13px; margin: 0 0 12px; }
.row { display: flex; gap: 8px; align-items: center; margin-top: 12px; }
.input { flex: 1; padding: 6px 10px; border: 1px solid #ccc; border-radius: 4px; }
.grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 12px;
}
label {
  display: flex;
  flex-direction: column;
  font-size: 13px;
  gap: 4px;
}
.field {
  display: flex;
  flex-direction: column;
  font-size: 13px;
  gap: 4px;
  margin-top: 12px;
}
input, select {
  padding: 6px 10px;
  border: 1px solid #ccc;
  border-radius: 4px;
  font-size: 14px;
}
button {
  padding: 6px 16px;
  border: 0;
  border-radius: 4px;
  background: #3b82f6;
  color: #fff;
  cursor: pointer;
}
button.secondary { background: #6b7280; }
.saved { color: #16a34a; font-size: 13px; }
</style>
