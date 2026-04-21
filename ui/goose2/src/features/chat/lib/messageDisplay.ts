import {
  getTextContent,
  type Message,
  type MessageContent,
} from "@/shared/types/messages";
import { getBuiltinSlashCommand } from "./slashCommands";

function getSlashEchoPrefixes(previousMessage: Message | null): string[] {
  if (!previousMessage || previousMessage.role !== "user") {
    return [];
  }

  const commandText = getTextContent(previousMessage).trim();
  const builtinCommand = getBuiltinSlashCommand(commandText);
  if (!builtinCommand) {
    return [];
  }

  return Array.from(new Set([commandText, builtinCommand.command])).sort(
    (left, right) => right.length - left.length,
  );
}

function stripLeadingSlashEcho(
  content: MessageContent[],
  prefixes: string[],
): MessageContent[] | null {
  if (prefixes.length === 0) {
    return null;
  }

  let sawFirstTextBlock = false;
  let didSanitize = false;
  const nextContent: MessageContent[] = [];

  for (const block of content) {
    if (block.type !== "text" || sawFirstTextBlock) {
      nextContent.push(block);
      continue;
    }

    sawFirstTextBlock = true;

    const matchingPrefix = prefixes.find((prefix) =>
      block.text.startsWith(prefix),
    );
    if (!matchingPrefix) {
      nextContent.push(block);
      continue;
    }

    didSanitize = true;
    const sanitizedText = block.text
      .slice(matchingPrefix.length)
      .replace(/^\s+/, "");

    if (sanitizedText.length > 0) {
      nextContent.push({ ...block, text: sanitizedText });
    }
  }

  return didSanitize ? nextContent : null;
}

function sanitizeAssistantSlashCommandEcho(
  message: Message,
  previousMessage: Message | null,
): Message {
  if (message.role !== "assistant") {
    return message;
  }

  const sanitizedContent = stripLeadingSlashEcho(
    message.content,
    getSlashEchoPrefixes(previousMessage),
  );
  if (!sanitizedContent) {
    return message;
  }

  return {
    ...message,
    content: sanitizedContent,
  };
}

export function sanitizeMessagesForDisplay(messages: Message[]): Message[] {
  return messages.map((message, index) =>
    sanitizeAssistantSlashCommandEcho(
      message,
      index > 0 ? messages[index - 1] : null,
    ),
  );
}
