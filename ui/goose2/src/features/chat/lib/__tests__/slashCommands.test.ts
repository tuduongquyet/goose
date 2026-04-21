import { describe, expect, it } from "vitest";
import type { Message } from "@/shared/types/messages";
import { removeMutatingSlashCommandUserMessages } from "../slashCommands";

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

describe("slashCommands", () => {
  it("removes mutating built-in slash commands from replayed histories", () => {
    const sanitizedMessages = removeMutatingSlashCommandUserMessages([
      createMessage("user-1", "user", "Before"),
      createMessage("user-2", "user", "/clear"),
      createMessage("user-3", "user", "/compact/compact"),
      createMessage("user-4", "user", "/skills"),
      createMessage("assistant-1", "assistant", "After"),
    ]);

    expect(sanitizedMessages).toEqual([
      createMessage("user-1", "user", "Before"),
      createMessage("user-4", "user", "/skills"),
      createMessage("assistant-1", "assistant", "After"),
    ]);
  });

  it("leaves unknown slash commands alone", () => {
    const sanitizedMessages = removeMutatingSlashCommandUserMessages([
      createMessage("user-1", "user", "/recipe release-notes"),
      createMessage("assistant-1", "assistant", "Generated release notes."),
    ]);

    expect(sanitizedMessages).toEqual([
      createMessage("user-1", "user", "/recipe release-notes"),
      createMessage("assistant-1", "assistant", "Generated release notes."),
    ]);
  });
});
