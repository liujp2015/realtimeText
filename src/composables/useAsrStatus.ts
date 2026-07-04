import { onBeforeUnmount, ref } from "vue";
import { onAsrStatus, type AsrStatus } from "@/lib/tauri";

export function useAsrStatus() {
  const connected = ref(false);
  const retryCount = ref(0);
  const lastError = ref<string | null>(null);

  let unlisten: (() => void) | null = null;

  (async () => {
    unlisten = await onAsrStatus((s: AsrStatus) => {
      connected.value = s.connected;
      retryCount.value = s.retry_count;
      lastError.value = s.last_error ?? null;
    });
  })();

  onBeforeUnmount(() => {
    if (unlisten) unlisten();
  });

  return { connected, retryCount, lastError };
}
