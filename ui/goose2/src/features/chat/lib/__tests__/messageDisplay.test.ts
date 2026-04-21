import { describe, expect, it } from "vitest";
import type { Message } from "@/shared/types/messages";
import { sanitizeMessagesForDisplay } from "../messageDisplay";

function createMessage(
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

describe("messageDisplay", () => {
  it("strips a built-in slash command echo from the following assistant message", () => {
    const [, assistantMessage] = sanitizeMessagesForDisplay([
      createMessage("user-1", "user", "/prompts"),
      createMessage(
        "assistant-1",
        "assistant",
        "/promptsNo prompts available.",
      ),
    ]);

    expect(assistantMessage.content).toEqual([
      { type: "text", text: "No prompts available." },
    ]);
  });

  it("prefers the full typed command when the assistant echoes command arguments", () => {
    const [, assistantMessage] = sanitizeMessagesForDisplay([
      createMessage("user-1", "user", "/prompt docs"),
      createMessage(
        "assistant-1",
        "assistant",
        "/prompt docsPrompt docs was not found.",
      ),
    ]);

    expect(assistantMessage.content).toEqual([
      { type: "text", text: "Prompt docs was not found." },
    ]);
  });

  it("leaves unknown slash commands unchanged", () => {
    const [, assistantMessage] = sanitizeMessagesForDisplay([
      createMessage("user-1", "user", "/recipe release-notes"),
      createMessage(
        "assistant-1",
        "assistant",
        "/recipe release-notesGenerated release notes.",
      ),
    ]);

    expect(assistantMessage.content).toEqual([
      {
        type: "text",
        text: "/recipe release-notesGenerated release notes.",
      },
    ]);
  });
});
