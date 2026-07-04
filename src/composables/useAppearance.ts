import { computed } from "vue";
import { useSettingsStore } from "@/stores/settings";

export function useAppearance() {
  const settings = useSettingsStore();

  const cssVars = computed(() => ({
    "--font-family": settings.appearance.font_family,
    "--font-size": `${settings.appearance.font_size}px`,
    "--text-color": settings.appearance.text_color,
    "--bg-opacity": settings.appearance.bg_opacity.toString(),
  }));

  return { cssVars, settings };
}
