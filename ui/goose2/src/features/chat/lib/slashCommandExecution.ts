import { acpLoadSession, acpSendMessage } from "@/shared/api/acp";
import { getGooseSessionId } from "@/shared/api/acpSessionTracker";
import {
  createSystemNotificationMessage,
  createUserMessage,
} from "@/shared/types/messages";
import {
  clearReplayBuffer,
  getAndDeleteReplayBuffer,
} from "../hooks/replayBuffer";
import { useChatStore, type ChatStore } from "../stores/chatStore";
import { getChatErrorMessage } from "./chatErrorMessage";
import {
  getBuiltinSlashCommand,
  removeMutatingSlashCommandUserMessages,
  type BuiltinSlashCommand,
} from "./slashCommands";

interface ExecuteSlashCommandOptions {
  sessionId: string;
  commandText: string;
  effectivePersonaInfo?: { id: string; name: string };
  builtinCommand?: BuiltinSlashCommand | null;
  showCommandMessage?: boolean;
  clearDraftWhenAccepted?: boolean;
  notifyMessageAccepted?: boolean;
  ensurePrepared?: () => Promise<void>;
  onMessageAccepted?: (sessionId: string) => void;
  getWorkingDir: () => string | undefined;
  setAbortController: (abortController: AbortController | null) => void;
  setStreamingPersonaId: (personaId: string | null) => void;
  store: ChatStore;
}

function getSlashCommandPreparationError(command: BuiltinSlashCommand): string {
  if (command.name === "compact") {
    return "Session not prepared. Send a message before compacting.";
  }

  return `Session not prepared. Send a message before running ${command.command}.`;
}

export async function executeSlashCommand({
  sessionId,
  commandText,
  effectivePersonaInfo,
  builtinCommand = getBuiltinSlashCommand(commandText),
  showCommandMessage = false,
  clearDraftWhenAccepted = false,
  notifyMessageAccepted = false,
  ensurePrepared,
  onMessageAccepted,
  getWorkingDir,
  setAbortController,
  setStreamingPersonaId,
  store,
}: ExecuteSlashCommandOptions): Promise<void> {
  const currentChatState = useChatStore
    .getState()
    .getSessionRuntime(sessionId).chatState;
  if (
    currentChatState === "streaming" ||
    currentChatState === "thinking" ||
    currentChatState === "compacting"
  ) {
    return;
  }

  const mutatesHistory = builtinCommand?.mutatesHistory ?? false;

  store.setActiveSession(sessionId);
  store.setChatState(sessionId, mutatesHistory ? "compacting" : "thinking");
  store.setStreamingMessageId(sessionId, null);
  store.setError(sessionId, null);

  if (showCommandMessage) {
    store.addMessage(sessionId, createUserMessage(commandText));
  }

  if (notifyMessageAccepted) {
    onMessageAccepted?.(sessionId);
  }

  if (clearDraftWhenAccepted) {
    store.clearDraft(sessionId);
  }

  let gooseSessionId: string | null = null;
  if (mutatesHistory) {
    gooseSessionId = getGooseSessionId(sessionId, effectivePersonaInfo?.id);
    if (!gooseSessionId && builtinCommand) {
      const errorMessage = getSlashCommandPreparationError(builtinCommand);
      store.addMessage(
        sessionId,
        createSystemNotificationMessage(errorMessage, "error"),
      );
      store.setError(sessionId, errorMessage);
      store.setChatState(sessionId, "idle");
      return;
    }

    store.setSessionLoading(sessionId, true);
    clearReplayBuffer(sessionId);
  }

  const abortController = new AbortController();
  setAbortController(abortController);
  setStreamingPersonaId(effectivePersonaInfo?.id ?? null);

  try {
    if (!mutatesHistory) {
      await ensurePrepared?.();
      store.setChatState(sessionId, "streaming");
    }

    const sendOptions = effectivePersonaInfo?.id
      ? { personaId: effectivePersonaInfo.id }
      : undefined;
    await acpSendMessage(sessionId, commandText, sendOptions);

    if (mutatesHistory && gooseSessionId) {
      clearReplayBuffer(sessionId);
      await acpLoadSession(sessionId, gooseSessionId, getWorkingDir());

      const replayedMessages = getAndDeleteReplayBuffer(sessionId) ?? [];
      store.setMessages(
        sessionId,
        removeMutatingSlashCommandUserMessages(replayedMessages),
      );
    }

    store.setChatState(sessionId, "idle");
    store.setStreamingMessageId(sessionId, null);
  } catch (error) {
    if (error instanceof DOMException && error.name === "AbortError") {
      store.setChatState(sessionId, "idle");
    } else {
      if (mutatesHistory) {
        clearReplayBuffer(sessionId);
      }

      const errorMessage = getChatErrorMessage(error);
      const liveStore = useChatStore.getState();
      liveStore.addMessage(
        sessionId,
        createSystemNotificationMessage(errorMessage, "error"),
      );
      liveStore.setError(sessionId, errorMessage);
      liveStore.setChatState(sessionId, "idle");
      liveStore.setStreamingMessageId(sessionId, null);
    }
  } finally {
    setAbortController(null);
    setStreamingPersonaId(null);
    store.setPendingAssistantProvider(sessionId, null);
    store.setSessionLoading(sessionId, false);
  }
}
