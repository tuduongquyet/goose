import { useCallback, useRef } from "react";
import { useChatStore } from "../stores/chatStore";
import { useChatSessionStore } from "../stores/chatSessionStore";
import {
  type ChatAttachmentDraft,
  createSystemNotificationMessage,
  createUserMessage,
} from "@/shared/types/messages";
import type { ChatState, TokenState } from "@/shared/types/chat";
import { acpSendMessage, acpCancelSession } from "@/shared/api/acp";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import {
  getSessionTitleFromDraft,
  isDefaultChatTitle,
} from "../lib/sessionTitle";
import { findLastIndex } from "@/shared/lib/arrays";
import { perfLog } from "@/shared/lib/perfLog";
import {
  buildAcpImages,
  buildAttachmentPromptPreamble,
  buildMessageAttachments,
} from "../lib/attachments";
import {
  getBuiltinSlashCommand,
  isSlashCommandText,
} from "../lib/slashCommands";
import { getChatErrorMessage } from "../lib/chatErrorMessage";
import { executeSlashCommand } from "../lib/slashCommandExecution";

function markMessageStopped(sessionId: string, messageId: string) {
  useChatStore.getState().updateMessage(sessionId, messageId, (message) => {
    if (
      message.metadata?.completionStatus === "completed" ||
      message.metadata?.completionStatus === "error" ||
      message.metadata?.completionStatus === "stopped"
    ) {
      return message;
    }

    return {
      ...message,
      metadata: {
        ...message.metadata,
        completionStatus: "stopped",
      },
      content: message.content.map((block) =>
        block.type === "toolRequest" && block.status === "executing"
          ? { ...block, status: "stopped" }
          : block,
      ),
    };
  });
}

/**
 * Hook for managing a chat session -- sending messages, handling streaming,
 * and managing chat lifecycle.
 */
export function useChat(
  sessionId: string,
  providerOverride?: string,
  systemPromptOverride?: string,
  personaInfo?: { id: string; name: string },
  options?: {
    onMessageAccepted?: (sessionId: string) => void;
    ensurePrepared?: () => Promise<void>;
  },
) {
  const store = useChatStore();
  const abortRef = useRef<AbortController | null>(null);
  const streamingPersonaIdRef = useRef<string | null>(null);

  const messages = store.messagesBySession[sessionId] ?? [];
  const { chatState, tokenState, error, streamingMessageId } =
    store.getSessionRuntime(sessionId);
  const isStreaming = chatState === "streaming" || streamingMessageId !== null;

  const getStreamingPersonaId = useCallback(() => {
    if (!streamingMessageId) {
      return null;
    }

    return (
      messages.find((message) => message.id === streamingMessageId)?.metadata
        ?.personaId ?? null
    );
  }, [messages, streamingMessageId]);

  const resolvePersonaInfo = useCallback(
    (overridePersonaId?: string, overridePersonaName?: string) => {
      if (overridePersonaId) {
        // Read the latest persona snapshot at call time so override lookups
        // still work even if the agent store changed after this hook rendered.
        const personaName =
          overridePersonaName ??
          useAgentStore.getState().getPersonaById(overridePersonaId)
            ?.displayName ??
          overridePersonaId;
        return { id: overridePersonaId, name: personaName };
      }

      return personaInfo;
    },
    [personaInfo],
  );

  const getWorkingDir = useCallback(
    () =>
      useChatSessionStore.getState().activeWorkspaceBySession[sessionId]?.path,
    [sessionId],
  );

  const sendMessage = useCallback(
    async (
      text: string,
      overridePersona?: { id: string; name?: string },
      attachments?: ChatAttachmentDraft[],
    ) => {
      const sid = sessionId.slice(0, 8);
      const tSendStart = performance.now();
      const trimmedText = text.trim();
      const hasAttachments = (attachments?.length ?? 0) > 0;
      const currentChatState = useChatStore
        .getState()
        .getSessionRuntime(sessionId).chatState;
      if (
        (!trimmedText && !hasAttachments) ||
        currentChatState === "streaming" ||
        currentChatState === "thinking" ||
        currentChatState === "compacting"
      )
        return;
      perfLog(
        `[perf:send] ${sid} useChat.sendMessage start (textLen=${text.length}, attachments=${attachments?.length ?? 0})`,
      );

      const effectivePersonaInfo = resolvePersonaInfo(
        overridePersona?.id,
        overridePersona?.name,
      );
      const agent = useAgentStore.getState().getActiveAgent();
      const providerId = providerOverride ?? agent?.provider ?? "goose";
      const systemPrompt =
        systemPromptOverride ?? agent?.systemPrompt ?? undefined;
      const builtinSlashCommand = getBuiltinSlashCommand(trimmedText);
      const isTopLevelSlashCommand = isSlashCommandText(trimmedText);

      if (isTopLevelSlashCommand) {
        await executeSlashCommand({
          sessionId,
          commandText: trimmedText,
          effectivePersonaInfo,
          builtinCommand: builtinSlashCommand,
          showCommandMessage: true,
          clearDraftWhenAccepted: true,
          notifyMessageAccepted: true,
          ensurePrepared: options?.ensurePrepared,
          onMessageAccepted: options?.onMessageAccepted,
          getWorkingDir,
          setAbortController: (abortController) => {
            abortRef.current = abortController;
          },
          setStreamingPersonaId: (personaId) => {
            streamingPersonaIdRef.current = personaId;
          },
          store,
        });
        return;
      }

      const images = buildAcpImages(attachments);

      // Ensure active session
      store.setActiveSession(sessionId);
      store.setPendingAssistantProvider(sessionId, providerId);

      // Create and add user message
      const userMessage = createUserMessage(
        trimmedText,
        buildMessageAttachments(attachments),
      );
      if (effectivePersonaInfo) {
        userMessage.metadata = {
          ...userMessage.metadata,
          targetPersonaId: effectivePersonaInfo.id,
          targetPersonaName: effectivePersonaInfo.name,
        };
      }
      // Embed image content blocks into the user message for local display
      if (images && images.length > 0) {
        for (const img of images) {
          userMessage.content.push({
            type: "image",
            source: {
              type: "base64",
              mediaType: img.mimeType,
              data: img.base64,
            },
          });
        }
      }
      store.addMessage(sessionId, userMessage);
      store.setChatState(sessionId, "thinking");
      store.setError(sessionId, null);

      const sessionStore = useChatSessionStore.getState();
      const session = sessionStore.getSession(sessionId);

      // Immediately set the session/sidebar title from the user's message when
      // the session still has the default placeholder.  This gives instant
      // feedback instead of waiting for acp:done or acp:session_info.
      // A better backend-generated title will overwrite this if it arrives
      // via the acp:session_info event.
      if (session && isDefaultChatTitle(session.title)) {
        sessionStore.updateSession(sessionId, {
          title: getSessionTitleFromDraft(trimmedText, attachments),
          updatedAt: new Date().toISOString(),
        });
      } else {
        sessionStore.updateSession(sessionId, {
          updatedAt: new Date().toISOString(),
        });
      }

      options?.onMessageAccepted?.(sessionId);

      store.clearDraft(sessionId);

      const abort = new AbortController();
      abortRef.current = abort;
      streamingPersonaIdRef.current = effectivePersonaInfo?.id ?? null;

      try {
        await options?.ensurePrepared?.();

        store.setChatState(sessionId, "streaming");
        // When images are present with no text, pass a single space so the ACP
        // driver doesn't send an empty text content block that goose rejects.
        const attachmentPromptPreamble =
          buildAttachmentPromptPreamble(attachments);
        const promptBody = trimmedText || (images?.length ? " " : text);
        const acpPrompt = `${attachmentPromptPreamble}${promptBody}`;
        const tAcp = performance.now();
        perfLog(
          `[perf:send] ${sid} → acpSendMessage (setup took ${(tAcp - tSendStart).toFixed(1)}ms)`,
        );
        await acpSendMessage(sessionId, acpPrompt, {
          systemPrompt,
          personaId: effectivePersonaInfo?.id,
          personaName: effectivePersonaInfo?.name,
          images: images?.map(
            (img) => [img.base64, img.mimeType] as [string, string],
          ),
        });
        perfLog(
          `[perf:send] ${sid} acpSendMessage returned after ${(performance.now() - tAcp).toFixed(1)}ms (total sendMessage ${(performance.now() - tSendStart).toFixed(1)}ms)`,
        );

        store.setChatState(sessionId, "idle");
        store.setStreamingMessageId(sessionId, null);
      } catch (err) {
        if (err instanceof DOMException && err.name === "AbortError") {
          store.setChatState(sessionId, "idle");
        } else {
          const errorMessage = getChatErrorMessage(err);
          const liveStore = useChatStore.getState();
          const { streamingMessageId } = liveStore.getSessionRuntime(sessionId);
          if (streamingMessageId) {
            liveStore.updateMessage(
              sessionId,
              streamingMessageId,
              (message) => ({
                ...message,
                metadata: {
                  ...message.metadata,
                  completionStatus: "error",
                },
              }),
            );
          }

          liveStore.addMessage(
            sessionId,
            createSystemNotificationMessage(errorMessage, "error"),
          );
          store.setError(sessionId, errorMessage);
          store.setChatState(sessionId, "idle");
          store.setStreamingMessageId(sessionId, null);
        }
        store.setPendingAssistantProvider(sessionId, null);
      } finally {
        abortRef.current = null;
        streamingPersonaIdRef.current = null;
      }
    },
    [
      sessionId,
      store,
      providerOverride,
      systemPromptOverride,
      resolvePersonaInfo,
      options,
      getWorkingDir,
    ],
  );

  const stopGeneration = useCallback(() => {
    abortRef.current?.abort();
    const activePersonaId =
      streamingPersonaIdRef.current ?? getStreamingPersonaId();
    const activeStreamingMessageId = useChatStore
      .getState()
      .getSessionRuntime(sessionId).streamingMessageId;

    store.setChatState(sessionId, "idle");
    store.setStreamingMessageId(sessionId, null);
    store.setPendingAssistantProvider(sessionId, null);
    // Cancel the backend ACP session to stop orphaned streaming events
    acpCancelSession(sessionId, activePersonaId ?? undefined)
      .then((wasCancelled) => {
        if (wasCancelled && activeStreamingMessageId) {
          markMessageStopped(sessionId, activeStreamingMessageId);
        }
      })
      .catch(() => {
        // Best-effort cancellation — ignore errors
      });
  }, [getStreamingPersonaId, store, sessionId]);

  const retryLastMessage = useCallback(async () => {
    const sessionMessages = store.messagesBySession[sessionId] ?? [];
    // Find the last user message
    const lastUserIndex = findLastIndex(
      sessionMessages,
      (m) => m.role === "user",
    );
    if (lastUserIndex === -1) return;

    const lastUserMessage = sessionMessages[lastUserIndex];

    // Remove all messages after (and including) the last assistant response
    const messagesToKeep = sessionMessages.slice(0, lastUserIndex);
    store.setMessages(sessionId, messagesToKeep);

    // Extract the text and resend
    const textContent = lastUserMessage.content.find((c) => c.type === "text");
    if (textContent && "text" in textContent) {
      const targetPersonaId = lastUserMessage.metadata?.targetPersonaId;
      const targetPersonaName = lastUserMessage.metadata?.targetPersonaName;
      await sendMessage(
        textContent.text,
        targetPersonaId
          ? { id: targetPersonaId, name: targetPersonaName }
          : undefined,
      );
    }
  }, [sessionId, store, sendMessage]);

  const clearChat = useCallback(() => {
    abortRef.current?.abort();
    store.clearMessages(sessionId);
    store.setChatState(sessionId, "idle");
    store.setStreamingMessageId(sessionId, null);
    store.setPendingAssistantProvider(sessionId, null);
  }, [sessionId, store]);

  const compactConversation = useCallback(async () => {
    await executeSlashCommand({
      sessionId,
      commandText: "/compact",
      effectivePersonaInfo: resolvePersonaInfo(),
      builtinCommand: getBuiltinSlashCommand("/compact"),
      ensurePrepared: options?.ensurePrepared,
      onMessageAccepted: options?.onMessageAccepted,
      getWorkingDir,
      setAbortController: (abortController) => {
        abortRef.current = abortController;
      },
      setStreamingPersonaId: (personaId) => {
        streamingPersonaIdRef.current = personaId;
      },
      store,
    });
  }, [getWorkingDir, options, resolvePersonaInfo, sessionId, store]);

  const stopStreaming = stopGeneration;

  return {
    messages,
    chatState: chatState as ChatState,
    tokenState: tokenState as TokenState,
    error,
    streamingMessageId,
    sendMessage,
    stopGeneration,
    stopStreaming,
    retryLastMessage,
    clearChat,
    compactConversation,
    isStreaming,
  };
}
