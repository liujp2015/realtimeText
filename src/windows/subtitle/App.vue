<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSubtitle } from "@/composables/useSubtitle";
import { useSettingsStore } from "@/stores/settings";
import { api } from "@/lib/tauri";

const { draft, finals } = useSubtitle();
const settings = useSettingsStore();

const isDev = import.meta.env.DEV;
// dev: 保持不穿透，方便右键开 DevTools 调试
// prod: 穿透 + forward，让字幕不挡下面的播放器，但 mousemove 仍能转发以探测拖把
const defaultPassthrough = !isDev;

const overlayRef = ref<HTMLElement | null>(null);
let unlistenMoved: (() => void) | null = null;
// 当前穿透状态缓存，避免每次 mousemove 都发 IPC
let cursorPassthrough = false;

async function setCursorPassthrough(passthrough: boolean) {
  if (cursorPassthrough === passthrough) return;
  cursorPassthrough = passthrough;
  const win = getCurrentWindow();
  try {
    if (passthrough) {
      await win.setIgnoreCursorEvents(true, { forward: true });
    } else {
      await win.setIgnoreCursorEvents(false);
    }
  } catch (e) {
    console.warn("setIgnoreCursorEvents", e);
  }
}

function pointInOverlay(x: number, y: number): boolean {
  const el = overlayRef.value;
  if (!el) return false;
  const r = el.getBoundingClientRect();
  return x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
}

// forward 模式下穿透区域也能收到 mousemove：进入字幕区时关闭穿透以便接收 mousedown，离开时恢复
function onOverlayMouseMove(e: MouseEvent) {
  void setCursorPassthrough(!pointInOverlay(e.clientX, e.clientY));
}

async function startDrag() {
  const win = getCurrentWindow();
  try {
    await win.startDragging();
  } catch (e) {
    console.warn("startDragging", e);
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

async function loadAppearance() {
  await settings.load();
}

onMounted(async () => {
  console.log("[subtitle] window mounted, loading appearance");
  await loadAppearance();
  console.log("[subtitle] appearance loaded", JSON.stringify(settings.appearance));
  const win = getCurrentWindow();
  // Restore window position if saved
  const rect = (await api.configGet("window")) as
    | { x: number; y: number; width: number; height: number }
    | null;
  if (rect) {
    try {
      await win.setPosition({ logical: { x: rect.x, y: rect.y } });
      await win.setSize({ logical: { width: rect.width, height: rect.height } });
    } catch (e) {
      console.warn("restore window pos", e);
    }
  }
  // 设置默认穿透状态（注意：需在 onMoved 注册前完成，避免恢复位置触发持久化）
  cursorPassthrough = !defaultPassthrough;
  await setCursorPassthrough(defaultPassthrough);
  // 原生拖动结束后才触发，此时窗口位置已稳定，可安全持久化
  unlistenMoved = await win.onMoved(() => {
    void persistWindowRect();
  });
});

onBeforeUnmount(() => {
  if (unlistenMoved) unlistenMoved();
  void persistWindowRect();
});
</script>

<template>
  <div
    ref="overlayRef"
    class="subtitle-overlay"
    :style="{
      fontFamily: settings.appearance.font_family,
      fontSize: settings.appearance.font_size + 'px',
      color: settings.appearance.text_color,
      background: 'rgba(0, 0, 0, ' + settings.appearance.bg_opacity + ')',
    }"
    @mousemove="onOverlayMouseMove"
    @mousedown="startDrag"
    title="拖动字幕框"
  >
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
  cursor: move;
  user-select: none;
}
</style>
