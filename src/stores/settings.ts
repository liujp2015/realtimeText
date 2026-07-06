import { defineStore } from "pinia";
import { ref } from "vue";
import { api, type Appearance } from "@/lib/tauri";

const DEFAULT_APPEARANCE: Appearance = {
  font_family: "Microsoft YaHei",
  font_size: 24,
  text_color: "#FFFFFF",
  bg_opacity: 0.5,
};

export const DEFAULT_VOLC_RESOURCE_ID = "volc.seedasr.sauc.duration";
export const DEFAULT_VOLC_URL =
  "wss://openspeech.bytedance.com/api/v3/plan/sauc/bigmodel_async";

export type AsrProvider = "stepfun" | "volc";

export const useSettingsStore = defineStore("settings", () => {
  const apiKey = ref<string>("");
  const appearance = ref<Appearance>({ ...DEFAULT_APPEARANCE });
  const provider = ref<AsrProvider>("stepfun");
  const volcApiKey = ref<string>("");
  const volcResourceId = ref<string>(DEFAULT_VOLC_RESOURCE_ID);
  const volcUrl = ref<string>(DEFAULT_VOLC_URL);
  const loaded = ref(false);

  async function load() {
    const key = (await api.configGet("api_key")) as string | null;
    apiKey.value = key ?? "";
    const ap = (await api.configGet("appearance")) as Appearance | null;
    if (ap) appearance.value = { ...DEFAULT_APPEARANCE, ...ap };
    const p = (await api.configGet("provider")) as AsrProvider | null;
    if (p === "stepfun" || p === "volc") provider.value = p;
    const vak = (await api.configGet("volc_api_key")) as string | null;
    volcApiKey.value = vak ?? "";
    const vrid = (await api.configGet("volc_resource_id")) as string | null;
    volcResourceId.value = vrid ?? DEFAULT_VOLC_RESOURCE_ID;
    const vu = (await api.configGet("volc_url")) as string | null;
    volcUrl.value = vu ?? DEFAULT_VOLC_URL;
    loaded.value = true;
  }

  async function saveApiKey(value: string) {
    apiKey.value = value;
    await api.configSet("api_key", value);
  }

  async function saveProvider(value: AsrProvider) {
    provider.value = value;
    await api.configSet("provider", value);
  }

  async function saveVolcApiKey(value: string) {
    volcApiKey.value = value;
    await api.configSet("volc_api_key", value);
  }

  async function saveVolcResourceId(value: string) {
    volcResourceId.value = value;
    await api.configSet("volc_resource_id", value);
  }

  async function saveVolcUrl(value: string) {
    volcUrl.value = value;
    await api.configSet("volc_url", value);
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
    provider,
    volcApiKey,
    volcResourceId,
    volcUrl,
    loaded,
    load,
    saveApiKey,
    saveProvider,
    saveVolcApiKey,
    saveVolcResourceId,
    saveVolcUrl,
    saveAppearance,
    resetAppearance,
  };
});
