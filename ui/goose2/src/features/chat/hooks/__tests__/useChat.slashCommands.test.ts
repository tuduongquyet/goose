import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Message } from "@/shared/types/messages";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useChatStore } from "../../stores/chatStore";
import { useChatSessionStore } from "../../stores/chatSessionStore";
import { clearReplayBuffer, ensureReplayBuffer } from "../replayBuffer";

const mockAcpSendMessage = vi.fn();
const mockAcpCancelSession = vi.fn();
const mockAcpLoadSession = vi.fn();
const mockAcpPrepareSession = vi.fn();
const mockAcpSetModel = vi.fn();
const mockGetGooseSessionId = vi.fn();

vi.mock("@/shared/api/acp", () => ({
  acpSendMessage: (...args: unknown[]) => mockAcpSendMessage(...args),
  acpCancelSession: (...args: unknown[]) => mockAcpCancelSession(...args),
  acpLoadSession: (...args: unknown[]) => mockAcpLoadSession(...args),
  acpPrepareSession: (...args: unknown[]) => mockAcpPrepareSession(...args),
  acpSetModel: (...args: unknown[]) => mockAcpSetModel(...args),
}));

vi.mock("@/shared/api/acpSessionTracker", () => ({
  getGooseSessionId: (...args: unknown[]) => mockGetGooseSessionId(...args),
}));

import { useChat } from "../useChat";

function createTextMessage(
  id: string,
  role: Message["role"],
  text: string,
): Message {
  return {
    id,
    role,
    created: 0,
    content: [{ type: "text", text }],
    metadata: {
      userVisible: true,
      agentVisible: role !== "system",
    },
  };
}

describe("useChat slash commands", () => {
  beforeEach(() => {
    mockAcpSendMessage.mockReset();
    mockAcpCancelSession.mockReset();
    mockAcpLoadSession.mockReset();
    mockAcpPrepareSession.mockReset();
    mockAcpSetModel.mockReset();
    mockGetGooseSessionId.mockReset();
    clearReplayBuffer("session-1");
    useChatStore.setState({
      messagesBySession: {},
      sessionStateById: {},
      activeSessionId: null,
      isConnected: true,
      loadingSessionIds: new Set<string>(),
    });
    useChatSessionStore.setState({
      sessions: [],
      activeSessionId: null,
      isLoading: false,
      contextPanelOpenBySession: {},
      activeWorkspaceBySession: {},
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
      ],
      personasLoading: false,
      agents: [],
      agentsLoading: false,
      activeAgentId: null,
      isLoading: false,
      personaEditorOpen: false,
      editingPersona: null,
    });
    mockAcpSendMessage.mockResolvedValue(undefined);
    mockAcpCancelSession.mockResolvedValue(true);
    mockAcpLoadSession.mockResolvedValue(undefined);
    mockAcpPrepareSession.mockResolvedValue(undefined);
    mockAcpSetModel.mockResolvedValue(undefined);
    mockGetGooseSessionId.mockReturnValue(null);
  });

  it("typed /compact uses the raw command path and reloads replayed history", async () => {
    mockGetGooseSessionId.mockReturnValue("goose-session-1");
    mockAcpLoadSession.mockImplementation(async (sessionId: string) => {
      const buffer = ensureReplayBuffer(sessionId);
      buffer.push(createTextMessage("user-1", "user", "Before compact"));
      buffer.push(createTextMessage("command-1", "user", "/compact"));
      buffer.push(
        createTextMessage("assistant-1", "assistant", "After compact"),
      );
    });

    useChatStore
      .getState()
      .setMessages("session-1", [
        createTextMessage("stale-1", "assistant", "Stale"),
      ]);

    const { result } = renderHook(() => useChat("session-1"));

    await act(async () => {
      await result.current.sendMessage("/compact");
    });

    expect(mockAcpSendMessage).toHaveBeenCalledWith(
      "session-1",
      "/compact",
      undefined,
    );
    expect(mockAcpLoadSession).toHaveBeenCalledWith(
      "session-1",
      "goose-session-1",
      undefined,
    );
    expect(useChatStore.getState().messagesBySession["session-1"]).toEqual([
      createTextMessage("user-1", "user", "Before compact"),
      createTextMessage("assistant-1", "assistant", "After compact"),
    ]);
  });

  it("typed /clear reloads history and drops stale local messages", async () => {
    mockGetGooseSessionId.mockReturnValue("goose-session-1");
    useChatStore
      .getState()
      .setMessages("session-1", [
        createTextMessage("stale-1", "assistant", "Stale"),
      ]);

    const { result } = renderHook(() => useChat("session-1"));

    await act(async () => {
      await result.current.sendMessage("/clear");
    });

    expect(mockAcpSendMessage).toHaveBeenCalledWith(
      "session-1",
      "/clear",
      undefined,
    );
    expect(mockAcpLoadSession).toHaveBeenCalledWith(
      "session-1",
      "goose-session-1",
      undefined,
    );
    expect(useChatStore.getState().messagesBySession["session-1"]).toEqual([]);
  });

  it("recognized built-ins ignore attachments and persona/system prompt wrappers", async () => {
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
        },
      ],
    });

    const { result } = renderHook(() =>
      useChat("session-1", "openai", "Wrap this command", {
        id: "persona-a",
        name: "Persona A",
      }),
    );

    await act(async () => {
      await result.current.sendMessage("/doctor", undefined, [
        {
          id: "file-1",
          kind: "file",
          name: "report.pdf",
          path: "/tmp/report.pdf",
          mimeType: "application/pdf",
        },
        {
          id: "image-1",
          kind: "image",
          name: "diagram.png",
          path: "/tmp/diagram.png",
          mimeType: "image/png",
          base64: "abc123",
          previewUrl: "tauri://localhost/tmp/diagram.png",
        },
      ]);
    });

    expect(mockAcpSendMessage).toHaveBeenCalledWith("session-1", "/doctor", {
      personaId: "persona-a",
    });

    const [message] = useChatStore.getState().messagesBySession["session-1"];
    expect(message.content).toEqual([{ type: "text", text: "/doctor" }]);
    expect(message.metadata?.attachments).toBeUndefined();
    expect(message.metadata?.targetPersonaId).toBeUndefined();
    expect(useChatSessionStore.getState().sessions[0]?.title).toBe("New Chat");
  });

  it("unknown slash commands stay on the raw send path without forcing a reload", async () => {
    const { result } = renderHook(() =>
      useChat("session-1", "openai", "Wrap this command", {
        id: "persona-a",
        name: "Persona A",
      }),
    );

    await act(async () => {
      await result.current.sendMessage("/recipe release-notes");
    });

    expect(mockAcpSendMessage).toHaveBeenCalledWith(
      "session-1",
      "/recipe release-notes",
      { personaId: "persona-a" },
    );
    expect(mockAcpLoadSession).not.toHaveBeenCalled();
  });
});
