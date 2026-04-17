import type { ChatState } from "@/shared/types/chat";

export function isSessionRunning(chatState: ChatState): boolean {
  return (
    chatState === "spinning_up" ||
    chatState === "thinking" ||
    chatState === "streaming" ||
    chatState === "waiting" ||
    chatState === "compacting"
  );
}
