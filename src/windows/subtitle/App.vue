<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSubtitle } from "@/composables/useSubtitle";
import { useSettingsStore } from "@/stores/settings";
import { api } from "@/lib/tauri";

const { draft, finals } = useSubtitle();
const settings = useSettingsStore();

const dragging = ref(false);

async function loadAppearance() {
  await settings.load();
}

onMounted(async () => {
  console.log("[subtitle] window mounted, loading appearance");
  await loadAppearance();
  console.log("[subtitle] appearance loaded", JSON.stringify(settings.appearance));
  // Restore window position if saved
  const rect = (await api.configGet("window")) as
    | { x: number; y: number; width: number; height: number }
    | null;
  if (rect) {
    const win = getCurrentWindow();
    try {
      await win.setPosition({ logical: { x: rect.x, y: rect.y } });
      await win.setSize({ logical: { width: rect.width, height: rect.height } });
    } catch (e) {
      console.warn("restore window pos", e);
    }
  }
});

async function startDrag() {
  dragging.value = true;
  const win = getCurrentWindow();
  try {
    await win.setIgnoreCursorEvents(false);
    await win.startDragging();
  } finally {
    dragging.value = false;
    try {
      await win.setIgnoreCursorEvents(true);
    } catch (e) {
      console.warn(e);
    }
    await persistWindowRect();
  }
}

async function persistWindowRect() {
  const win = getCurrentWindow();
  try {
    const factor = await win.scaleFactor();
    const pos = await win.outerPosition();
    const size = await win.outerSize();
    await api.configSet("window", {
      x: pos.x / factor,
      y: pos.y / factor,
      width: size.width / factor,
      height: size.height / factor,
    });
  } catch (e) {
    console.warn("persist window rect", e);
  }
}

onBeforeUnmount(() => {
  void persistWindowRect();
});
</script>

<template>
  <div
    class="subtitle-overlay"
    :style="{
      fontFamily: settings.appearance.font_family,
      fontSize: settings.appearance.font_size + 'px',
      color: settings.appearance.text_color,
      background: 'rgba(0, 0, 0, ' + settings.appearance.bg_opacity + ')',
    }"
  >
    <div class="drag-handle" @mousedown.stop="startDrag">≡</div>
    <div v-if="!draft && finals.length === 0" class="subtitle-line draft">
      [等待字幕...]
    </div>
    <div v-for="(f, i) in finals.slice(-5)" :key="i" class="subtitle-line final">
      {{ f.text }}
    </div>
    <div v-if="draft" class="subtitle-line draft">{{ draft }}</div>
  </div>
</template>

<style scoped>
.subtitle-overlay {
  width: 100%;
  padding: 8px 16px;
  box-sizing: border-box;
}
</style>
