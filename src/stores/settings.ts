import { defineStore } from "pinia";
import { ref, watch } from "vue";
import { api, type Appearance } from "@/lib/tauri";

const DEFAULT_APPEARANCE: Appearance = {
  font_family: "Microsoft YaHei",
  font_size: 24,
  text_color: "#FFFFFF",
  bg_opacity: 0.5,
};

export const useSettingsStore = defineStore("settings", () => {
  const apiKey = ref<string>("");
  const appearance = ref<Appearance>({ ...DEFAULT_APPEARANCE });
  const loaded = ref(false);

  async function load() {
    const key = (await api.configGet("api_key")) as string | null;
    apiKey.value = key ?? "";
    const ap = (await api.configGet("appearance")) as Appearance | null;
    if (ap) appearance.value = { ...DEFAULT_APPEARANCE, ...ap };
    loaded.value = true;
  }

  async function saveApiKey(value: string) {
    apiKey.value = value;
    await api.configSet("api_key", value);
  }

  async function saveAppearance(value: Appearance) {
    appearance.value = value;
    await api.configSet("appearance", value);
  }

  async function resetAppearance() {
    const def = await api.configResetAppearance();
    appearance.value = def;
  }

  return {
    apiKey,
    appearance,
    loaded,
    load,
    saveApiKey,
    saveAppearance,
    resetAppearance,
  };
});
