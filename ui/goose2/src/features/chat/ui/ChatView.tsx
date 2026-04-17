import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { AnimatePresence } from "motion/react";
import { MessageTimeline } from "./MessageTimeline";
import { ChatInput } from "./ChatInput";
import type { ChatAttachmentDraft } from "@/shared/types/messages";
import { LoadingGoose } from "./LoadingGoose";
import { ChatLoadingSkeleton } from "./ChatLoadingSkeleton";
import { useChat } from "../hooks/useChat";
import { useMessageQueue } from "../hooks/useMessageQueue";
import { rebuildAttachmentDrafts } from "../lib/attachments";
import { useChatStore } from "../stores/chatStore";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useProviderSelection } from "@/features/agents/hooks/useProviderSelection";
import { useChatSessionStore } from "../stores/chatSessionStore";
import { useProjectStore } from "@/features/projects/stores/projectStore";
import { acpPrepareSession, acpSetModel } from "@/shared/api/acp";
import {
  buildProjectSystemPrompt,
  composeSystemPrompt,
  defaultArtifactsDir,
  getProjectArtifactRoots,
  resolveProjectWorkingDir,
} from "@/features/projects/lib/chatProjectContext";
import { getHomeDir } from "@/shared/api/system";
import { ArtifactPolicyProvider } from "../hooks/ArtifactPolicyContext";
import type { ModelOption } from "../types";
import { ChatContextPanel } from "./ChatContextPanel";

const EMPTY_MODELS: ModelOption[] = [];

interface ChatViewProps {
  sessionId: string;
  initialProvider?: string;
  initialPersonaId?: string;
  initialMessage?: string;
  initialAttachments?: ChatAttachmentDraft[];
  onInitialMessageConsumed?: () => void;
  onCreateProject?: (options?: {
    onCreated?: (projectId: string) => void;
  }) => void;
}

export function ChatView({
  sessionId,
  initialProvider,
  initialPersonaId,
  initialMessage,
  initialAttachments,
  onInitialMessageConsumed,
  onCreateProject,
}: ChatViewProps) {
  const { t } = useTranslation("chat");
  const activeSessionId = sessionId;
  const isContextPanelOpen = useChatSessionStore(
    (s) => s.contextPanelOpenBySession[activeSessionId] ?? false,
  );
  const setContextPanelOpen = useChatSessionStore((s) => s.setContextPanelOpen);
  const activeWorkingContext = useChatSessionStore(
    (s) => s.activeWorkingContextBySession[activeSessionId],
  );
  const clearActiveWorkingContext = useChatSessionStore(
    (s) => s.clearActiveWorkingContext,
  );

  const {
    providers,
    providersLoading,
    selectedProvider: globalSelectedProvider,
    setSelectedProvider: setGlobalSelectedProvider,
  } = useProviderSelection();
  const personas = useAgentStore((s) => s.personas);
  const [selectedPersonaId, setSelectedPersonaId] = useState<string | null>(
    initialPersonaId ?? null,
  );
  const session = useChatSessionStore((s) =>
    s.sessions.find((candidate) => candidate.id === activeSessionId),
  );
  const availableModels = useChatSessionStore(
    (s) => s.modelsBySession[activeSessionId] ?? EMPTY_MODELS,
  );
  const projects = useProjectStore((s) => s.projects);
  const projectsLoading = useProjectStore((s) => s.loading);
  const storedProject = useProjectStore((s) =>
    session?.projectId
      ? s.projects.find((candidate) => candidate.id === session.projectId)
      : undefined,
  );
  const [homeArtifactsRoot, setHomeArtifactsRoot] = useState<string | null>(
    null,
  );
  const project = storedProject ?? null;
  const contextPanelLabel = isContextPanelOpen
    ? t("context.closePanel")
    : t("context.openPanel");
  const availableProjects = useMemo(
    () =>
      [...projects]
        .sort((a, b) => a.order - b.order || a.name.localeCompare(b.name))
        .map((projectInfo) => ({
          id: projectInfo.id,
          name: projectInfo.name,
          workingDirs: projectInfo.workingDirs,
          color: projectInfo.color,
        })),
    [projects],
  );
  const selectedProvider =
    session?.providerId ??
    initialProvider ??
    project?.preferredProvider ??
    globalSelectedProvider;

  const selectedPersona = personas.find((p) => p.id === selectedPersonaId);
  const projectArtifactRoots = useMemo(
    () => getProjectArtifactRoots(project),
    [project],
  );
  const resolvedProjectWorkingDir = useMemo(
    () => resolveProjectWorkingDir(project),
    [project],
  );
  const projectMetadataPending = Boolean(
    session?.projectId && !resolvedProjectWorkingDir && projectsLoading,
  );
  const defaultWorkingDir = resolvedProjectWorkingDir
    ? resolvedProjectWorkingDir
    : !session?.projectId
      ? (homeArtifactsRoot ?? undefined)
      : undefined;
  const effectiveWorkingDir = activeWorkingContext?.path ?? defaultWorkingDir;
  const allowedArtifactRoots = useMemo(() => {
    const roots = [
      ...projectArtifactRoots.map((path) => path.trim()).filter(Boolean),
    ];
    if (homeArtifactsRoot) {
      roots.push(homeArtifactsRoot);
    }
    return [...new Set(roots)];
  }, [homeArtifactsRoot, projectArtifactRoots]);
  const projectSystemPrompt = useMemo(
    () => buildProjectSystemPrompt(project),
    [project],
  );
  const workingContextPrompt = useMemo(() => {
    if (!activeWorkingContext?.branch) return undefined;
    return `<active-working-context>\nActive branch: ${activeWorkingContext.branch}\nWorking directory: ${activeWorkingContext.path}\n</active-working-context>`;
  }, [activeWorkingContext?.branch, activeWorkingContext?.path]);

  const effectiveSystemPrompt = useMemo(
    () =>
      composeSystemPrompt(
        selectedPersona?.systemPrompt,
        projectSystemPrompt,
        workingContextPrompt,
      ),
    [selectedPersona?.systemPrompt, projectSystemPrompt, workingContextPrompt],
  );

  useEffect(() => {
    let cancelled = false;
    getHomeDir()
      .then((homeDir) => {
        if (cancelled) return;
        setHomeArtifactsRoot(defaultArtifactsDir(homeDir));
      })
      .catch(() => {
        if (cancelled) return;
        setHomeArtifactsRoot(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const prevProjectIdRef = useRef(session?.projectId);
  useEffect(() => {
    const prevProjectId = prevProjectIdRef.current;
    prevProjectIdRef.current = session?.projectId;
    if (prevProjectId !== undefined && prevProjectId !== session?.projectId) {
      clearActiveWorkingContext(activeSessionId);
    }
  }, [session?.projectId, activeSessionId, clearActiveWorkingContext]);

  const prevContextRef = useRef(activeWorkingContext);
  useEffect(() => {
    const prev = prevContextRef.current;
    if (
      !activeWorkingContext ||
      !selectedProvider ||
      session?.draft ||
      activeWorkingContext === prev
    ) {
      return;
    }
    prevContextRef.current = activeWorkingContext;
    if (prev && prev.path === activeWorkingContext.path) return;
    void acpPrepareSession(activeSessionId, selectedProvider, {
      workingDir: activeWorkingContext.path,
      personaId: selectedPersonaId ?? undefined,
    }).catch((error) => {
      console.error("Failed to prepare ACP session:", error);
    });
  }, [
    activeWorkingContext,
    activeSessionId,
    selectedProvider,
    selectedPersonaId,
    session?.draft,
  ]);

  const handleProviderChange = useCallback(
    (providerId: string) => {
      if (providerId === selectedProvider) {
        return;
      }
      const sessionStore = useChatSessionStore.getState();
      const cached = sessionStore.getCachedModels(providerId);
      sessionStore.switchSessionProvider(activeSessionId, providerId, cached);
      setGlobalSelectedProvider(providerId);
    },
    [activeSessionId, selectedProvider, setGlobalSelectedProvider],
  );

  const handleProjectChange = useCallback(
    (projectId: string | null) => {
      const nextProject =
        projectId == null
          ? null
          : (useProjectStore
              .getState()
              .projects.find((candidate) => candidate.id === projectId) ??
            null);
      const nextWorkingDir =
        resolveProjectWorkingDir(nextProject) ??
        (projectId == null ? (homeArtifactsRoot ?? undefined) : undefined);

      useChatSessionStore
        .getState()
        .updateSession(activeSessionId, { projectId });

      if (!session?.draft && selectedProvider && nextWorkingDir) {
        void acpPrepareSession(activeSessionId, selectedProvider, {
          workingDir: nextWorkingDir,
          personaId: selectedPersonaId ?? undefined,
        }).catch((error) => {
          console.error(
            "Failed to update ACP session working directory:",
            error,
          );
        });
      }
    },
    [
      activeSessionId,
      homeArtifactsRoot,
      selectedPersonaId,
      selectedProvider,
      session?.draft,
    ],
  );
  const handleModelChange = useCallback(
    (modelId: string) => {
      if (!activeSessionId || modelId === session?.modelId) {
        return;
      }
      const previousModelId = session?.modelId;
      const previousModelName = session?.modelName;
      const models = useChatSessionStore
        .getState()
        .getSessionModels(activeSessionId);
      const selected = models.find((m) => m.id === modelId);
      useChatSessionStore.getState().updateSession(activeSessionId, {
        modelId,
        modelName: selected?.displayName ?? selected?.name ?? modelId,
      });
      if (session?.draft) {
        return;
      }
      acpSetModel(activeSessionId, modelId).catch((error) => {
        console.error("Failed to set model:", error);
        useChatSessionStore.getState().updateSession(activeSessionId, {
          modelId: previousModelId,
          modelName: previousModelName,
        });
      });
    },
    [activeSessionId, session?.draft, session?.modelId, session?.modelName],
  );

  // When persona changes, update the provider to match persona's default
  const handlePersonaChange = useCallback(
    (personaId: string | null) => {
      setSelectedPersonaId(personaId);
      const persona = personas.find((p) => p.id === personaId);
      if (persona?.provider) {
        const matchingProvider = providers.find(
          (p) =>
            p.id === persona.provider ||
            p.label.toLowerCase().includes(persona.provider ?? ""),
        );
        if (matchingProvider) {
          handleProviderChange(matchingProvider.id);
        }
      }
      const agentStore = useAgentStore.getState();
      const matchingAgent = agentStore.agents.find(
        (a) => a.personaId === personaId,
      );
      if (matchingAgent) {
        agentStore.setActiveAgent(matchingAgent.id);
      }
      useChatSessionStore
        .getState()
        .updateSession(activeSessionId, { personaId: personaId ?? undefined });
    },
    [personas, providers, activeSessionId, handleProviderChange],
  );

  // Validate persona still exists — fall back to default if deleted
  useEffect(() => {
    if (
      selectedPersonaId !== null &&
      personas.length > 0 &&
      !personas.find((p) => p.id === selectedPersonaId)
    ) {
      // Selected persona was deleted — reset to no persona
      setSelectedPersonaId(null);
    }
  }, [personas, selectedPersonaId]);

  const personaInfo = selectedPersona
    ? { id: selectedPersona.id, name: selectedPersona.displayName }
    : undefined;
  const {
    messages,
    chatState,
    tokenState,
    sendMessage,
    stopStreaming,
    retryMessage,
    editMessage,
    cancelEdit,
    editingMessageId,
    streamingMessageId,
  } = useChat(
    activeSessionId,
    selectedProvider,
    effectiveSystemPrompt,
    personaInfo,
    effectiveWorkingDir,
  );
  const isLoadingHistory = useChatStore(
    (s) =>
      s.loadingSessionIds.has(activeSessionId) &&
      (s.messagesBySession[activeSessionId]?.length ?? 0) === 0,
  );

  // Unified deferred send — a single ref + effect drains any pending send
  // that had to wait for a state update (persona switch or edit truncation).
  const pendingSend = useRef<{
    text: string;
    attachments?: ChatAttachmentDraft[];
  } | null>(null);
  const queue = useMessageQueue(activeSessionId, chatState, sendMessage);
  const chatStore = useChatStore();
  const handleSend = useCallback(
    (text: string, personaId?: string, attachments?: ChatAttachmentDraft[]) => {
      if (personaId && personaId !== selectedPersonaId) {
        const newPersona = personas.find((p) => p.id === personaId);
        if (newPersona) {
          // Inject a system notification about the persona switch
          chatStore.addMessage(activeSessionId, {
            id: crypto.randomUUID(),
            role: "system",
            created: Date.now(),
            content: [
              {
                type: "systemNotification",
                notificationType: "info",
                text: `Switched to ${newPersona.displayName}`,
              },
            ],
            metadata: { userVisible: true, agentVisible: false },
          });
        }
        handlePersonaChange(personaId);
        // Defer the send until after persona state updates
        pendingSend.current = { text, attachments };
        return;
      }
      // Queue if agent is busy and no message already queued
      if (chatState !== "idle" && !queue.queuedMessage) {
        queue.enqueue(text, personaId, attachments);
        return;
      }

      sendMessage(text, undefined, attachments);
    },
    [
      sendMessage,
      selectedPersonaId,
      handlePersonaChange,
      personas,
      chatStore,
      activeSessionId,
      chatState,
      queue,
    ],
  );

  /** Save an inline edit: truncate history from the edited message onward, then send the new text. */
  /** Save an inline edit: truncate locally for instant UI feedback, then
   *  send with truncation metadata so the backend also rewinds both the
   *  display log and LLM context.
   *
   *  When the user edits while streaming, stopStreaming() sets chatState
   *  to "idle" in the store, but sendMessage's closure still captures the
   *  stale "streaming" value and silently bails. We defer the send into a
   *  ref so the next render — which carries the fresh "idle" chatState and
   *  a new sendMessage — picks it up via the useEffect below. */
  const pendingEditSend = useRef<{
    text: string;
    truncateMessageId: string;
    persona?: { id: string; name?: string };
    attachments?: ChatAttachmentDraft[];
  } | null>(null);
  const handleSaveEdit = useCallback(
    (messageId: string, text: string) => {
      const wasIdle = chatState === "idle";
      if (!wasIdle) {
        stopStreaming();
      }
      const store = useChatStore.getState();
      const allMessages = store.messagesBySession[activeSessionId] ?? [];
      const editIndex = allMessages.findIndex((m) => m.id === messageId);
      if (editIndex === -1) {
        store.setEditingMessageId(activeSessionId, null);
        return;
      }
      const originalMessage = allMessages[editIndex];
      const targetPersonaId = originalMessage.metadata?.targetPersonaId;
      const targetPersonaName = originalMessage.metadata?.targetPersonaName;
      const originalAttachments = rebuildAttachmentDrafts(originalMessage);
      const persona = targetPersonaId
        ? { id: targetPersonaId, name: targetPersonaName }
        : undefined;
      const attachments =
        originalAttachments.length > 0 ? originalAttachments : undefined;

      // Local truncation for immediate UI feedback.
      store.setMessages(activeSessionId, allMessages.slice(0, editIndex));
      store.setEditingMessageId(activeSessionId, null);
      // Dismiss any queued follow-up before forcing idle — otherwise
      // useMessageQueue would auto-send it against the truncated timeline.
      store.dismissQueuedMessage(activeSessionId);
      store.setChatState(activeSessionId, "idle");

      if (wasIdle) {
        // chatState was already "idle" — sendMessage's closure is fresh.
        sendMessage(text, persona, attachments, messageId);
      } else {
        // Defer until React re-renders with fresh chatState / sendMessage.
        pendingEditSend.current = {
          text,
          truncateMessageId: messageId,
          persona,
          attachments,
        };
      }
    },
    [activeSessionId, chatState, stopStreaming, sendMessage],
  );

  useEffect(() => {
    if (pendingEditSend.current && chatState === "idle") {
      const { text, truncateMessageId, persona, attachments } =
        pendingEditSend.current;
      pendingEditSend.current = null;
      sendMessage(text, persona, attachments, truncateMessageId);
    }
  }, [chatState, sendMessage]);

  // Drain deferred sends (persona switch only).
  useEffect(() => {
    if (pendingSend.current && selectedPersona) {
      const { text, attachments } = pendingSend.current;
      pendingSend.current = null;
      sendMessage(text, undefined, attachments);
    }
  }, [sendMessage, selectedPersona]);
  const initialMessageSent = useRef(false);
  useEffect(() => {
    if (
      (initialMessage || initialAttachments?.length) &&
      !initialMessageSent.current
    ) {
      initialMessageSent.current = true;
      handleSend(initialMessage ?? "", undefined, initialAttachments);
      onInitialMessageConsumed?.();
    }
  }, [
    initialAttachments,
    initialMessage,
    handleSend,
    onInitialMessageConsumed,
  ]);
  const isStreaming = chatState === "streaming";
  const showIndicator =
    chatState === "thinking" ||
    chatState === "streaming" ||
    chatState === "waiting" ||
    chatState === "compacting";
  const handleCreatePersona = useCallback(() => {
    useAgentStore.getState().openPersonaEditor();
  }, []);
  const draftValue = useChatStore(
    (s) => s.draftsBySession[activeSessionId] ?? "",
  );
  const scrollTarget = useChatStore(
    (s) => s.scrollTargetMessageBySession[activeSessionId] ?? null,
  );
  const handleDraftChange = useCallback(
    (text: string) => {
      useChatStore.getState().setDraft(activeSessionId, text);
    },
    [activeSessionId],
  );
  const handleScrollTargetHandled = useCallback(() => {
    useChatStore.getState().clearScrollTargetMessage(activeSessionId);
  }, [activeSessionId]);
  return (
    <ArtifactPolicyProvider
      messages={messages}
      allowedRoots={allowedArtifactRoots}
    >
      <div className="relative flex h-full min-w-0">
        <div className="flex min-w-0 flex-1 flex-col pr-1">
          {isLoadingHistory ? (
            <ChatLoadingSkeleton />
          ) : (
            <MessageTimeline
              messages={messages}
              streamingMessageId={streamingMessageId}
              scrollTargetMessageId={scrollTarget?.messageId ?? null}
              scrollTargetQuery={scrollTarget?.query ?? null}
              onScrollTargetHandled={handleScrollTargetHandled}
              onRetryMessage={retryMessage}
              onEditMessage={editMessage}
              editingMessageId={editingMessageId}
              onSaveEdit={handleSaveEdit}
              onCancelEdit={cancelEdit}
            />
          )}

          <AnimatePresence initial={false}>
            {showIndicator && !isLoadingHistory ? (
              <LoadingGoose
                key="loading-indicator"
                chatState={
                  chatState as
                    | "thinking"
                    | "streaming"
                    | "waiting"
                    | "compacting"
                }
              />
            ) : null}
          </AnimatePresence>

          <ChatInput
            onSend={handleSend}
            disabled={projectMetadataPending}
            queuedMessage={queue.queuedMessage}
            onDismissQueue={queue.dismiss}
            initialValue={draftValue}
            onDraftChange={handleDraftChange}
            onStop={stopStreaming}
            isStreaming={isStreaming || chatState === "thinking"}
            personas={personas}
            selectedPersonaId={selectedPersonaId}
            onPersonaChange={handlePersonaChange}
            onCreatePersona={handleCreatePersona}
            providers={providers}
            providersLoading={providersLoading}
            selectedProvider={selectedProvider}
            onProviderChange={handleProviderChange}
            currentModelId={session?.modelId ?? null}
            currentModel={session?.modelName}
            availableModels={availableModels}
            onModelChange={handleModelChange}
            selectedProjectId={session?.projectId ?? null}
            availableProjects={availableProjects}
            onProjectChange={handleProjectChange}
            onCreateProject={(options) =>
              onCreateProject?.({
                onCreated: (projectId) => {
                  handleProjectChange(projectId);
                  options?.onCreated?.(projectId);
                },
              })
            }
            contextTokens={tokenState.accumulatedTotal}
            contextLimit={tokenState.contextLimit}
          />
        </div>

        <ChatContextPanel
          activeSessionId={activeSessionId}
          isOpen={isContextPanelOpen}
          label={contextPanelLabel}
          project={project}
          setOpen={setContextPanelOpen}
        />
      </div>
    </ArtifactPolicyProvider>
  );
}
