import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ChatInput } from "../ChatInput";

const mockVoiceDictation = {
  isEnabled: true,
  isRecording: false,
  isTranscribing: false,
  isStarting: vi.fn(() => false),
  stopRecording: vi.fn(),
  toggleRecording: vi.fn(),
};

vi.mock("../hooks/useVoiceDictation", () => ({
  useVoiceDictation: () => mockVoiceDictation,
}));

vi.mock("@/features/providers/hooks/useAgentProviderStatus", () => ({
  useAgentProviderStatus: () => ({
    readyAgentIds: new Set(["goose", "claude-acp", "codex-acp"]),
    loading: false,
    refresh: vi.fn(),
  }),
}));

const mockListFilesForMentions = vi.fn<
  (roots: string[], maxResults?: number) => Promise<string[]>
>(async () => []);

vi.mock("@/shared/api/system", () => ({
  listFilesForMentions: (roots: string[], maxResults?: number) =>
    mockListFilesForMentions(roots, maxResults),
}));

describe("ChatInput slash commands", () => {
  beforeEach(() => {
    mockListFilesForMentions.mockReset();
    mockListFilesForMentions.mockResolvedValue([]);
    mockVoiceDictation.isEnabled = true;
    mockVoiceDictation.isRecording = false;
    mockVoiceDictation.isTranscribing = false;
    mockVoiceDictation.isStarting.mockReset();
    mockVoiceDictation.isStarting.mockReturnValue(false);
    mockVoiceDictation.stopRecording.mockReset();
    mockVoiceDictation.toggleRecording.mockReset();
  });

  it("renders placeholder text that advertises slash commands", () => {
    render(<ChatInput onSend={vi.fn()} />);

    expect(
      screen.getByPlaceholderText(
        "Message Goose, @ to mention personas, / for commands",
      ),
    ).toBeInTheDocument();
  });

  it("opens the slash command picker when typing /", async () => {
    const user = userEvent.setup();
    render(<ChatInput onSend={vi.fn()} />);

    await user.type(screen.getByRole("textbox"), "/");

    expect(
      screen.getByRole("listbox", { name: "Slash command suggestions" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Commands")).toBeInTheDocument();
    expect(screen.getByText("/prompts")).toBeInTheDocument();
  });

  it("filters the slash command picker by command name", async () => {
    const user = userEvent.setup();
    render(<ChatInput onSend={vi.fn()} />);

    await user.type(screen.getByRole("textbox"), "/cl");

    expect(screen.getByText("/clear")).toBeInTheDocument();
    expect(screen.queryByText("/compact")).not.toBeInTheDocument();
  });

  it("sends the selected command immediately on mouse click", async () => {
    const user = userEvent.setup();
    const onSend = vi.fn();
    render(<ChatInput onSend={onSend} />);

    const input = screen.getByRole("textbox");
    await user.type(input, "/cl");
    await user.click(screen.getByRole("option", { name: /\/clear/i }));

    expect(onSend).toHaveBeenCalledWith("/clear", undefined, undefined);
    expect(input).toHaveValue("");
  });

  it("sends the highlighted command immediately on Enter", async () => {
    const onSend = vi.fn();
    const user = userEvent.setup();
    render(<ChatInput onSend={onSend} />);

    const input = screen.getByRole("textbox");
    await user.type(input, "/cl");
    await user.keyboard("{Enter}");

    expect(onSend).toHaveBeenCalledWith("/clear", undefined, undefined);
    expect(input).toHaveValue("");
  });
});
