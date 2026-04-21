import { getTextContent, type Message } from "@/shared/types/messages";

export interface BuiltinSlashCommand {
  name: string;
  command: `/${string}`;
  label: string;
  description: string;
  order: number;
  mutatesHistory: boolean;
}

export const BUILTIN_SLASH_COMMANDS: BuiltinSlashCommand[] = [
  {
    name: "prompts",
    command: "/prompts",
    label: "List prompts",
    description: "List available prompts, optionally filtered by extension.",
    order: 10,
    mutatesHistory: false,
  },
  {
    name: "prompt",
    command: "/prompt",
    label: "Run a prompt",
    description: "Execute a prompt or show its info with --info.",
    order: 20,
    mutatesHistory: false,
  },
  {
    name: "compact",
    command: "/compact",
    label: "Compact conversation",
    description: "Compact the conversation history.",
    order: 30,
    mutatesHistory: true,
  },
  {
    name: "clear",
    command: "/clear",
    label: "Clear conversation",
    description: "Clear the conversation history.",
    order: 40,
    mutatesHistory: true,
  },
  {
    name: "skills",
    command: "/skills",
    label: "List skills",
    description: "List installed skills and other available sources.",
    order: 50,
    mutatesHistory: false,
  },
  {
    name: "doctor",
    command: "/doctor",
    label: "Run doctor",
    description: "Check that your Goose setup is working.",
    order: 60,
    mutatesHistory: false,
  },
];

const BUILTIN_SLASH_COMMANDS_BY_NAME = new Map(
  BUILTIN_SLASH_COMMANDS.map((command) => [command.name, command]),
);

export function getLeadingSlashCommandName(
  text: string | null | undefined,
): string | null {
  if (!text) {
    return null;
  }

  const trimmed = text.trim();
  if (!trimmed.startsWith("/")) {
    return null;
  }

  const match = /^\/([^\s]*)/.exec(trimmed);
  if (!match) {
    return null;
  }

  return match[1].toLowerCase();
}

export function isSlashCommandText(text: string | null | undefined): boolean {
  return getLeadingSlashCommandName(text) !== null;
}

export function getBuiltinSlashCommand(
  text: string | null | undefined,
): BuiltinSlashCommand | null {
  const commandName = getLeadingSlashCommandName(text);
  if (!commandName) {
    return null;
  }

  return BUILTIN_SLASH_COMMANDS_BY_NAME.get(commandName) ?? null;
}

export function filterBuiltinSlashCommands(
  query: string,
): BuiltinSlashCommand[] {
  const normalizedQuery = query.trim().replace(/^\/+/, "").toLowerCase();

  return BUILTIN_SLASH_COMMANDS.filter((command) => {
    if (!normalizedQuery) {
      return true;
    }

    return (
      command.name.includes(normalizedQuery) ||
      command.label.toLowerCase().includes(normalizedQuery) ||
      command.description.toLowerCase().includes(normalizedQuery)
    );
  }).sort((left, right) => left.order - right.order);
}

export function isSlashCommandUserMessage(
  message: Message,
  commandName: string,
): boolean {
  if (message.role !== "user") {
    return false;
  }

  const normalizedText = getTextContent(message).replace(/\s+/g, "");
  if (!normalizedText) {
    return false;
  }

  const commandText = `/${commandName}`;
  return (
    normalizedText.startsWith(commandText) &&
    normalizedText.replaceAll(commandText, "").length === 0
  );
}

export function removeSlashCommandUserMessages(
  messages: Message[],
  commandName: string,
): Message[] {
  return messages.filter(
    (message) => !isSlashCommandUserMessage(message, commandName),
  );
}
