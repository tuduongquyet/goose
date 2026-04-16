import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useChatStore } from "../../stores/chatStore";
import { useChatSessionStore } from "../../stores/chatSessionStore";
import type { Message } from "@/shared/types/messages";

const mockAcpSendMessage = vi.fn();
const mockAcpCancelSession = vi.fn();
const mockAcpPrepareSession = vi.fn();
const mockAcpSetModel = vi.fn();

vi.mock("@/shared/api/acp", () => ({
  acpSendMessage: (...args: unknown[]) => mockAcpSendMessage(...args),
  acpCancelSession: (...args: unknown[]) => mockAcpCancelSession(...args),
  acpPrepareSession: (...args: unknown[]) => mockAcpPrepareSession(...args),
  acpSetModel: (...args: unknown[]) => mockAcpSetModel(...args),
}));

import { useChat } from "../useChat";

function addStreamingAssistantMessage(
  sessionId: string,
  messageId: string,
  personaId: string,
  personaName: string,
) {
  const message: Message = {
    id: messageId,
    role: "assistant",
    created: Date.now(),
    content: [],
    metadata: {
      userVisible: true,
      agentVisible: true,
      personaId,
      personaName,
      completionStatus: "inProgress",
    },
  };

  useChatStore.getState().addMessage(sessionId, message);
  useChatStore.getState().setStreamingMessageId(sessionId, messageId);
}

function createDeferredPromise<T = void>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

describe("useChat", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useChatStore.setState({
      messagesBySession: {},
      sessionStateById: {},
      activeSessionId: null,
      isConnected: true,
      editingMessageIdBySession: {},
      draftsBySession: {},
    });
    useChatSessionStore.setState({
      sessions: [],
      activeSessionId: null,
      isLoading: false,
      contextPanelOpenBySession: {},
      activeWorkingContextBySession: {},
      modelsBySession: {},
      modelCacheByProvider: {},
    });
    useAgentStore.setState({
      personas: [
        {
          id: "persona-a",
          displayName: "Persona A",
          systemPrompt: "",
          isBuiltin: false,
          createdAt: "",
          updatedAt: "",
        },
        {
          id: "persona-b",
          displayName: "Persona B",
          systemPrompt: "",
          isBuiltin: false,
          createdAt: "",
          updatedAt: "",
        },
      ],
      personasLoading: false,
      agents: [],
      agentsLoading: false,
      activeAgentId: null,
      isLoading: false,
      personaEditorOpen: false,
      editingPersona: null,
    });
    mockAcpCancelSession.mockResolvedValue(true);
    mockAcpPrepareSession.mockResolvedValue(undefined);
    mockAcpSetModel.mockResolvedValue(undefined);
  });

  it("cancels the active override persona instead of the hook default persona", async () => {
    const deferred = createDeferredPromise();
    mockAcpSendMessage.mockReturnValue(deferred.promise);

    const { result } = renderHook(() =>
      useChat("session-1", undefined, undefined, {
        id: "persona-a",
        name: "Persona A",
      }),
    );

    let sendPromise!: Promise<void>;
    await act(async () => {
      sendPromise = result.current.sendMessage("Hello", {
        id: "persona-b",
        name: "Persona B",
      });
      await Promise.resolve();
    });

    act(() => {
      result.current.stopGeneration();
    });

    expect(mockAcpSendMessage).toHaveBeenCalledWith("session-1", "Hello", {
      systemPrompt: undefined,
      personaId: "persona-b",
      personaName: "Persona B",
      images: undefined,
    });
    expect(mockAcpCancelSession).toHaveBeenCalledWith("session-1", "persona-b");

    deferred.resolve();
    await act(async () => {
      await sendPromise;
    });
  });

  it("keeps persona-aware cancellation working after remount", async () => {
    const deferred = createDeferredPromise();
    mockAcpSendMessage.mockReturnValue(deferred.promise);

    const firstMount = renderHook(() =>
      useChat("session-1", undefined, undefined, {
        id: "persona-a",
        name: "Persona A",
      }),
    );

    let sendPromise!: Promise<void>;
    await act(async () => {
      sendPromise = firstMount.result.current.sendMessage("Hello", {
        id: "persona-b",
        name: "Persona B",
      });
      await Promise.resolve();
    });
    act(() => {
      addStreamingAssistantMessage(
        "session-1",
        "assistant-1",
        "persona-b",
        "Persona B",
      );
    });

    act(() => {
      firstMount.unmount();
    });

    const secondMount = renderHook(() =>
      useChat("session-1", undefined, undefined, {
        id: "persona-a",
        name: "Persona A",
      }),
    );

    act(() => {
      secondMount.result.current.stopGeneration();
    });

    expect(mockAcpCancelSession).toHaveBeenCalledWith("session-1", "persona-b");

    deferred.resolve();
    await act(async () => {
      await sendPromise;
    });
  });

  it("marks the streaming message stopped only after cancellation succeeds", async () => {
    const cancelDeferred = createDeferredPromise<boolean>();
    mockAcpCancelSession.mockReturnValue(cancelDeferred.promise);

    const { result } = renderHook(() => useChat("session-1"));

    act(() => {
      addStreamingAssistantMessage(
        "session-1",
        "assistant-1",
        "persona-a",
        "Persona A",
      );
      useChatStore.getState().setChatState("session-1", "streaming");
    });

    act(() => {
      result.current.stopGeneration();
    });

    let message = useChatStore.getState().messagesBySession["session-1"][0];
    const runtime = useChatStore.getState().getSessionRuntime("session-1");

    expect(message.metadata?.completionStatus).toBe("inProgress");
    expect(runtime.chatState).toBe("idle");
    expect(runtime.streamingMessageId).toBeNull();

    await act(async () => {
      cancelDeferred.resolve(true);
      await cancelDeferred.promise;
    });

    message = useChatStore.getState().messagesBySession["session-1"][0];
    expect(message.metadata?.completionStatus).toBe("stopped");
  });

  it("does not overwrite a completed message when stop loses the race", async () => {
    const cancelDeferred = createDeferredPromise<boolean>();
    mockAcpCancelSession.mockReturnValue(cancelDeferred.promise);

    const { result } = renderHook(() => useChat("session-1"));

    act(() => {
      addStreamingAssistantMessage(
        "session-1",
        "assistant-1",
        "persona-a",
        "Persona A",
      );
      useChatStore.getState().setChatState("session-1", "streaming");
    });

    act(() => {
      result.current.stopGeneration();
      useChatStore
        .getState()
        .updateMessage("session-1", "assistant-1", (message) => ({
          ...message,
          metadata: {
            ...message.metadata,
            completionStatus: "completed",
          },
        }));
    });

    await act(async () => {
      cancelDeferred.resolve(true);
      await cancelDeferred.promise;
    });

    const message = useChatStore.getState().messagesBySession["session-1"][0];
    expect(message.metadata?.completionStatus).toBe("completed");
  });

  it("does not mark the message stopped when cancellation reports no active session", async () => {
    mockAcpCancelSession.mockResolvedValue(false);

    const { result } = renderHook(() => useChat("session-1"));

    act(() => {
      addStreamingAssistantMessage(
        "session-1",
        "assistant-1",
        "persona-a",
        "Persona A",
      );
      useChatStore.getState().setChatState("session-1", "streaming");
    });

    await act(async () => {
      result.current.stopGeneration();
      await Promise.resolve();
    });

    const message = useChatStore.getState().messagesBySession["session-1"][0];
    expect(message.metadata?.completionStatus).toBe("inProgress");
  });

  it("allows another session to send while a different session is streaming", async () => {
    const deferred = createDeferredPromise();
    mockAcpSendMessage
      .mockReturnValueOnce(deferred.promise)
      .mockResolvedValueOnce(undefined);

    const firstSession = renderHook(() => useChat("session-1"));
    const secondSession = renderHook(() => useChat("session-2"));

    let firstPromise!: Promise<void>;
    await act(async () => {
      firstPromise = firstSession.result.current.sendMessage("First");
      await Promise.resolve();
    });

    await act(async () => {
      await secondSession.result.current.sendMessage("Second");
    });

    expect(mockAcpSendMessage).toHaveBeenNthCalledWith(
      1,
      "session-1",
      "First",
      {
        systemPrompt: undefined,
        personaId: undefined,
        personaName: undefined,
        images: undefined,
      },
    );
    expect(mockAcpSendMessage).toHaveBeenNthCalledWith(
      2,
      "session-2",
      "Second",
      {
        systemPrompt: undefined,
        personaId: undefined,
        personaName: undefined,
        images: undefined,
      },
    );

    deferred.resolve();
    await act(async () => {
      await firstPromise;
    });
  });

  it("prepares draft sessions before applying a selected model on first send", async () => {
    useChatSessionStore.setState({
      sessions: [
        {
          id: "session-1",
          title: "New Chat",
          providerId: "openai",
          modelId: "gpt-4.1",
          modelName: "GPT-4.1",
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
          messageCount: 0,
          draft: true,
        },
      ],
    });

    const { result } = renderHook(() => useChat("session-1", "openai"));

    await act(async () => {
      await result.current.sendMessage("Hello");
    });

    expect(mockAcpPrepareSession).toHaveBeenCalledWith("session-1", "openai", {
      workingDir: undefined,
      personaId: undefined,
    });
    expect(mockAcpSetModel).toHaveBeenCalledWith("session-1", "gpt-4.1");
    expect(mockAcpSendMessage).toHaveBeenCalledWith("session-1", "Hello", {
      systemPrompt: undefined,
      personaId: undefined,
      personaName: undefined,
      images: undefined,
    });
  });

  it("appends an error message and removes the empty assistant placeholder when send fails", async () => {
    mockAcpSendMessage.mockRejectedValue(
      new Error("Working directory missing"),
    );

    const { result } = renderHook(() => useChat("session-1"));

    await act(async () => {
      await result.current.sendMessage("Hello");
    });

    const messages = useChatStore.getState().messagesBySession["session-1"];
    const runtime = useChatStore.getState().getSessionRuntime("session-1");

    expect(messages).toHaveLength(2);
    expect(messages[0].role).toBe("user");
    expect(messages[1].role).toBe("system");
    expect(messages[1].content).toEqual([
      {
        type: "systemNotification",
        notificationType: "error",
        text: "Working directory missing",
      },
    ]);
    expect(runtime.error).toBe("Working directory missing");
    expect(runtime.streamingMessageId).toBeNull();
    expect(runtime.chatState).toBe("idle");
  });

  it("shows string-shaped invoke errors instead of falling back to unknown error", async () => {
    mockAcpSendMessage.mockRejectedValue("Working directory missing");

    const { result } = renderHook(() => useChat("session-1"));

    await act(async () => {
      await result.current.sendMessage("Hello");
    });

    const messages = useChatStore.getState().messagesBySession["session-1"];
    expect(messages[1].content).toEqual([
      {
        type: "systemNotification",
        notificationType: "error",
        text: "Working directory missing",
      },
    ]);
  });

  // ---------------------------------------------------------------------------
  // retryMessage
  // ---------------------------------------------------------------------------

  describe("retryMessage", () => {
    function seedConversation(
      sessionId: string,
      msgs: Array<{
        id: string;
        role: "user" | "assistant";
        text: string;
        personaId?: string;
        personaName?: string;
      }>,
    ) {
      const messages: Message[] = msgs.map((m) => ({
        id: m.id,
        role: m.role,
        created: Date.now(),
        content: [{ type: "text", text: m.text }],
        metadata: {
          userVisible: true,
          agentVisible: true,
          ...(m.personaId
            ? { targetPersonaId: m.personaId, targetPersonaName: m.personaName }
            : {}),
        },
      }));
      useChatStore.getState().setMessages(sessionId, messages);
    }

    it("truncates from target user message and re-sends its text", async () => {
      mockAcpSendMessage.mockResolvedValue(undefined);

      seedConversation("session-1", [
        { id: "u1", role: "user", text: "First question" },
        { id: "a1", role: "assistant", text: "First answer" },
        { id: "u2", role: "user", text: "Second question" },
        { id: "a2", role: "assistant", text: "Second answer" },
      ]);

      const { result } = renderHook(() => useChat("session-1"));

      await act(async () => {
        await result.current.retryMessage("u2");
      });

      // History should be truncated to just the first exchange, then a new user
      // message appended by sendMessage.
      const messages = useChatStore.getState().messagesBySession["session-1"];
      // u1, a1 survive the truncation; sendMessage adds a new user message
      expect(messages.length).toBeGreaterThanOrEqual(3);
      expect(messages[0].id).toBe("u1");
      expect(messages[1].id).toBe("a1");
      // The third message is the re-sent user message (new id)
      expect(messages[2].role).toBe("user");
      expect(messages[2].content[0]).toEqual({
        type: "text",
        text: "Second question",
      });

      expect(mockAcpSendMessage).toHaveBeenCalledWith(
        "session-1",
        "Second question",
        expect.objectContaining({ personaId: undefined }),
      );
    });

    it("preserves persona when retrying a persona-targeted message", async () => {
      mockAcpSendMessage.mockResolvedValue(undefined);

      seedConversation("session-1", [
        {
          id: "u1",
          role: "user",
          text: "Hello persona",
          personaId: "persona-a",
          personaName: "Persona A",
        },
        { id: "a1", role: "assistant", text: "Hi there" },
      ]);

      const { result } = renderHook(() => useChat("session-1"));

      await act(async () => {
        await result.current.retryMessage("u1");
      });

      expect(mockAcpSendMessage).toHaveBeenCalledWith(
        "session-1",
        "Hello persona",
        expect.objectContaining({
          personaId: "persona-a",
          personaName: "Persona A",
        }),
      );
    });

    it("handles retrying an assistant message by finding the preceding user message", async () => {
      mockAcpSendMessage.mockResolvedValue(undefined);

      seedConversation("session-1", [
        { id: "u1", role: "user", text: "My question" },
        { id: "a1", role: "assistant", text: "My answer" },
      ]);

      const { result } = renderHook(() => useChat("session-1"));

      await act(async () => {
        await result.current.retryMessage("a1");
      });

      // Truncation should start from u1 (the preceding user message)
      const messages = useChatStore.getState().messagesBySession["session-1"];
      // Only the re-sent user message should remain (u1 and a1 were truncated)
      expect(messages[0].role).toBe("user");
      expect(messages[0].content[0]).toEqual({
        type: "text",
        text: "My question",
      });
      expect(mockAcpSendMessage).toHaveBeenCalledTimes(1);
    });

    it("guards against retrying while streaming", async () => {
      seedConversation("session-1", [
        { id: "u1", role: "user", text: "Hello" },
        { id: "a1", role: "assistant", text: "World" },
      ]);
      useChatStore.getState().setChatState("session-1", "streaming");

      const { result } = renderHook(() => useChat("session-1"));

      await act(async () => {
        await result.current.retryMessage("u1");
      });

      // Messages should be untouched — retry was blocked
      const messages = useChatStore.getState().messagesBySession["session-1"];
      expect(messages).toHaveLength(2);
      expect(mockAcpSendMessage).not.toHaveBeenCalled();
    });

    it("guards against retrying while thinking", async () => {
      seedConversation("session-1", [
        { id: "u1", role: "user", text: "Hello" },
      ]);
      useChatStore.getState().setChatState("session-1", "thinking");

      const { result } = renderHook(() => useChat("session-1"));

      await act(async () => {
        await result.current.retryMessage("u1");
      });

      const messages = useChatStore.getState().messagesBySession["session-1"];
      expect(messages).toHaveLength(1);
      expect(mockAcpSendMessage).not.toHaveBeenCalled();
    });
  });

  // ---------------------------------------------------------------------------
  // editMessage
  // ---------------------------------------------------------------------------

  describe("editMessage", () => {
    it("sets editing state for a user message", () => {
      const userMsg: Message = {
        id: "u1",
        role: "user",
        created: Date.now(),
        content: [{ type: "text", text: "Hello" }],
        metadata: { userVisible: true, agentVisible: true },
      };
      useChatStore.getState().setMessages("session-1", [userMsg]);

      const { result } = renderHook(() => useChat("session-1"));

      act(() => {
        result.current.editMessage("u1");
      });

      expect(result.current.editingMessageId).toBe("u1");
    });

    it("guards against entering edit mode while streaming", () => {
      const userMsg: Message = {
        id: "u1",
        role: "user",
        created: Date.now(),
        content: [{ type: "text", text: "Hello" }],
        metadata: { userVisible: true, agentVisible: true },
      };
      useChatStore.getState().setMessages("session-1", [userMsg]);
      useChatStore.getState().setChatState("session-1", "streaming");

      const { result } = renderHook(() => useChat("session-1"));

      act(() => {
        result.current.editMessage("u1");
      });

      expect(result.current.editingMessageId).toBeNull();
    });

    it("refuses to edit an assistant message", () => {
      const assistantMsg: Message = {
        id: "a1",
        role: "assistant",
        created: Date.now(),
        content: [{ type: "text", text: "Hello" }],
        metadata: { userVisible: true, agentVisible: true },
      };
      useChatStore.getState().setMessages("session-1", [assistantMsg]);

      const { result } = renderHook(() => useChat("session-1"));

      act(() => {
        result.current.editMessage("a1");
      });

      expect(result.current.editingMessageId).toBeNull();
    });
  });

  // ---------------------------------------------------------------------------
  // cancelEdit
  // ---------------------------------------------------------------------------

  // ---------------------------------------------------------------------------
  // retryMessage — attachment & image-only edge cases
  // ---------------------------------------------------------------------------

  describe("retryMessage edge cases", () => {
    it("does not truncate history when retrying an image-only message (no text)", async () => {
      // Seed a user message that has only image content, no text
      const imageOnlyMsg: Message = {
        id: "img-1",
        role: "user",
        created: Date.now(),
        content: [
          {
            type: "image",
            source: {
              type: "base64",
              mediaType: "image/png",
              data: "iVBORw0KGgo=",
            },
          },
        ],
        metadata: { userVisible: true, agentVisible: true },
      };
      const assistantReply: Message = {
        id: "a1",
        role: "assistant",
        created: Date.now(),
        content: [{ type: "text", text: "I see the image" }],
        metadata: { userVisible: true, agentVisible: true },
      };
      useChatStore
        .getState()
        .setMessages("session-1", [imageOnlyMsg, assistantReply]);
      mockAcpSendMessage.mockResolvedValue(undefined);

      const { result } = renderHook(() => useChat("session-1"));

      await act(async () => {
        await result.current.retryMessage("img-1");
      });

      // The image-only message should be retried (not silently dropped).
      // sendMessage should have been called with the image attachment.
      expect(mockAcpSendMessage).toHaveBeenCalledTimes(1);
      // The re-sent message should carry the image
      expect(mockAcpSendMessage).toHaveBeenCalledWith(
        "session-1",
        expect.any(String),
        expect.objectContaining({
          images: [[expect.any(String), "image/png"]],
        }),
      );
    });

    it("preserves file attachments when retrying a message", async () => {
      mockAcpSendMessage.mockResolvedValue(undefined);

      const msgWithAttachments: Message = {
        id: "u1",
        role: "user",
        created: Date.now(),
        content: [{ type: "text", text: "Check this file" }],
        metadata: {
          userVisible: true,
          agentVisible: true,
          attachments: [
            { type: "file", name: "report.pdf", path: "/tmp/report.pdf" },
            { type: "directory", name: "src", path: "/tmp/src" },
          ],
        },
      };
      useChatStore.getState().setMessages("session-1", [msgWithAttachments]);

      const { result } = renderHook(() => useChat("session-1"));

      await act(async () => {
        await result.current.retryMessage("u1");
      });

      // The re-sent user message should carry the original attachments
      const messages = useChatStore.getState().messagesBySession["session-1"];
      const reSent = messages.find((m) => m.role === "user");
      expect(reSent?.metadata?.attachments).toEqual([
        expect.objectContaining({
          type: "file",
          name: "report.pdf",
          path: "/tmp/report.pdf",
        }),
        expect.objectContaining({
          type: "directory",
          name: "src",
          path: "/tmp/src",
        }),
      ]);
    });

    it("preserves image content blocks when retrying a message with text and images", async () => {
      mockAcpSendMessage.mockResolvedValue(undefined);

      const msgWithImage: Message = {
        id: "u1",
        role: "user",
        created: Date.now(),
        content: [
          { type: "text", text: "Look at this" },
          {
            type: "image",
            source: {
              type: "base64",
              mediaType: "image/jpeg",
              data: "base64data",
            },
          },
        ],
        metadata: { userVisible: true, agentVisible: true },
      };
      useChatStore.getState().setMessages("session-1", [msgWithImage]);

      const { result } = renderHook(() => useChat("session-1"));

      await act(async () => {
        await result.current.retryMessage("u1");
      });

      expect(mockAcpSendMessage).toHaveBeenCalledWith(
        "session-1",
        expect.stringContaining("Look at this"),
        expect.objectContaining({
          images: [["base64data", "image/jpeg"]],
        }),
      );
    });
  });

  describe("cancelEdit", () => {
    it("clears editing state but preserves compose draft", () => {
      const userMsg: Message = {
        id: "u1",
        role: "user",
        created: Date.now(),
        content: [{ type: "text", text: "Hello" }],
        metadata: { userVisible: true, agentVisible: true },
      };
      useChatStore.getState().setMessages("session-1", [userMsg]);
      useChatStore.getState().setEditingMessageId("session-1", "u1");
      useChatStore.getState().setDraft("session-1", "unsent compose text");

      const { result } = renderHook(() => useChat("session-1"));

      expect(result.current.editingMessageId).toBe("u1");

      act(() => {
        result.current.cancelEdit();
      });

      expect(result.current.editingMessageId).toBeNull();
      // cancelEdit must NOT wipe the compose draft — only editing state
      expect(useChatStore.getState().draftsBySession["session-1"]).toBe(
        "unsent compose text",
      );
    });
  });
});
