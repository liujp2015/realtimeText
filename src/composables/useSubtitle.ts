import { onBeforeUnmount, onMounted, ref } from "vue";
import { onSubtitleUpdate, type SubtitleUpdate } from "@/lib/tauri";

export function useSubtitle() {
  const draft = ref<string>("");
  const finals = ref<SubtitleUpdate[]>([]);

  let unlisten: (() => void) | null = null;

  onMounted(async () => {
    try {
      console.log("[subtitle] registering listener");
      unlisten = await onSubtitleUpdate((u: SubtitleUpdate) => {
        console.log("[subtitle] event", u.state, JSON.stringify(u.text));
        if (u.state === "partial") {
          draft.value = u.text;
        } else if (u.state === "final") {
          if (draft.value) {
            finals.value.push({
              state: "final",
              text: draft.value,
              start_ts: u.start_ts,
              end_ts: u.end_ts,
            });
          }
          finals.value.push(u);
          draft.value = "";
          if (finals.value.length > 50) {
            finals.value.splice(0, finals.value.length - 50);
          }
        }
      });
      console.log("[subtitle] listener registered");
    } catch (e) {
      console.error("[subtitle] listener failed", e);
    }
  });

  onBeforeUnmount(() => {
    if (unlisten) unlisten();
  });

  return { draft, finals };
}
