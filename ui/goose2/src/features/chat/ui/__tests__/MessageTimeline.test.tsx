import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { MessageTimeline } from "../MessageTimeline";
import type { Message } from "@/shared/types/messages";

function createMessage(
  id: string,
  role: Message["role"],
  text: string,
  overrides: Partial<Message> = {},
): Message {
  return {
    id,
    role,
    created: Date.now(),
    content: [{ type: "text", text }],
    metadata: {
      userVisible: true,
      agentVisible: role !== "system",
      ...(role === "assistant"
        ? { completionStatus: "completed" as const }
        : {}),
    },
    ...overrides,
  };
}

describe("MessageTimeline", () => {
  beforeEach(() => {
    HTMLElement.prototype.scrollTo = vi.fn();
  });

  it("renders built-in slash command responses without the echoed command prefix", () => {
    const { container } = render(
      <MessageTimeline
        messages={[
          createMessage("user-1", "user", "/prompts"),
          createMessage(
            "assistant-1",
            "assistant",
            "/promptsNo prompts available.",
          ),
        ]}
      />,
    );

    expect(screen.getByText("No prompts available.")).toBeInTheDocument();
    expect(
      container.querySelector('[data-role="assistant-message"]')?.textContent,
    ).not.toContain("/promptsNo prompts available.");
  });

  it("hides an in-progress assistant message when it only contains the echoed slash command", () => {
    const { container } = render(
      <MessageTimeline
        messages={[
          createMessage("user-1", "user", "/prompts"),
          createMessage("assistant-1", "assistant", "/prompts", {
            metadata: {
              userVisible: true,
              agentVisible: true,
              completionStatus: "inProgress",
            },
          }),
        ]}
        streamingMessageId="assistant-1"
      />,
    );

    expect(
      container.querySelector('[data-role="assistant-message"]'),
    ).toBeNull();
    expect(
      container.querySelector('[data-role="user-message"]'),
    ).toBeInTheDocument();
  });
});
