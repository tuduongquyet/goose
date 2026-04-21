import { useCallback } from "react";
import type { BuiltinSlashCommand } from "../lib/slashCommands";
import type { MentionItem } from "../ui/MentionAutocomplete";

interface ChatInputAutocompleteHandlersOptions {
  mentionOpen: boolean;
  detectMention: (value: string, cursorPosition: number) => void;
  closeMention: () => void;
  navigateMention: (direction: "up" | "down") => void;
  confirmMention: () => MentionItem | null;
  handleMentionConfirm: (item: MentionItem) => void;
  slashCommandOpen: boolean;
  detectSlashCommand: (value: string, cursorPosition: number) => void;
  closeSlashCommand: () => void;
  navigateSlashCommand: (direction: "up" | "down") => void;
  confirmSlashCommand: () => BuiltinSlashCommand | null;
  handleSlashCommandSelect: (command: BuiltinSlashCommand) => void;
  handleSend: () => void;
  setText: (value: string) => void;
}

export function useChatInputAutocompleteHandlers({
  mentionOpen,
  detectMention,
  closeMention,
  navigateMention,
  confirmMention,
  handleMentionConfirm,
  slashCommandOpen,
  detectSlashCommand,
  closeSlashCommand,
  navigateSlashCommand,
  confirmSlashCommand,
  handleSlashCommandSelect,
  handleSend,
  setText,
}: ChatInputAutocompleteHandlersOptions) {
  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      if (slashCommandOpen) {
        if (event.key === "Escape") {
          event.preventDefault();
          closeSlashCommand();
          return;
        }
        if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault();
          navigateSlashCommand(event.key === "ArrowDown" ? "down" : "up");
          return;
        }
        if (event.key === "Enter" || event.key === "Tab") {
          const command = confirmSlashCommand();
          if (command) {
            event.preventDefault();
            handleSlashCommandSelect(command);
            return;
          }
        }
      }

      if (mentionOpen) {
        if (event.key === "Escape") {
          event.preventDefault();
          closeMention();
          return;
        }
        if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault();
          navigateMention(event.key === "ArrowDown" ? "down" : "up");
          return;
        }
        if (event.key === "Enter" || event.key === "Tab") {
          const item = confirmMention();
          if (item) {
            event.preventDefault();
            handleMentionConfirm(item);
            return;
          }
        }
      }

      if (event.key === "Enter" && !event.shiftKey && !event.altKey) {
        event.preventDefault();
        handleSend();
      }
    },
    [
      closeMention,
      closeSlashCommand,
      confirmMention,
      confirmSlashCommand,
      handleMentionConfirm,
      handleSend,
      handleSlashCommandSelect,
      mentionOpen,
      navigateMention,
      navigateSlashCommand,
      slashCommandOpen,
    ],
  );

  const handleInput = useCallback(
    (event: React.ChangeEvent<HTMLTextAreaElement>) => {
      const value = event.target.value;
      setText(value);
      const cursorPosition = event.target.selectionStart ?? value.length;
      if (value.startsWith("/")) {
        closeMention();
        detectSlashCommand(value, cursorPosition);
      } else {
        closeSlashCommand();
        detectMention(value, cursorPosition);
      }

      const textarea = event.target;
      textarea.style.height = "auto";
      textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
    },
    [
      closeMention,
      closeSlashCommand,
      detectMention,
      detectSlashCommand,
      setText,
    ],
  );

  const handleSelectionChange = useCallback(
    (event: React.SyntheticEvent<HTMLTextAreaElement>) => {
      const textarea = event.currentTarget;
      const value = textarea.value;
      const cursorPosition = textarea.selectionStart ?? value.length;

      if (value.startsWith("/")) {
        detectSlashCommand(value, cursorPosition);
        return;
      }

      closeSlashCommand();
    },
    [closeSlashCommand, detectSlashCommand],
  );

  const handleBlur = useCallback(
    (event: React.FocusEvent<HTMLTextAreaElement>) => {
      const nextFocusedElement = event.relatedTarget;
      if (
        nextFocusedElement instanceof Element &&
        nextFocusedElement.closest("[role='listbox']")
      ) {
        return;
      }

      closeSlashCommand();
    },
    [closeSlashCommand],
  );

  return {
    handleBlur,
    handleKeyDown,
    handleInput,
    handleSelectionChange,
  };
}
