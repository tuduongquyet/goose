import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  filterBuiltinSlashCommands,
  type BuiltinSlashCommand,
} from "../lib/slashCommands";

interface SlashCommandAutocompleteOptions {
  text: string;
  setText: (value: string) => void;
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
}

function createClosedState() {
  return {
    isOpen: false,
    query: "",
    selectedIndex: 0,
  };
}

export function useSlashCommandAutocomplete({
  text,
  setText,
  textareaRef,
}: SlashCommandAutocompleteOptions) {
  const [slashState, setSlashState] = useState(createClosedState);
  const pendingCursorRef = useRef<number | null>(null);

  const filteredCommands = useMemo(
    () => filterBuiltinSlashCommands(slashState.query),
    [slashState.query],
  );

  // biome-ignore lint/correctness/useExhaustiveDependencies: text triggers the effect after setText flushes
  useEffect(() => {
    if (pendingCursorRef.current == null) {
      return;
    }

    const textarea = textareaRef.current;
    if (!textarea) {
      return;
    }

    const cursorPosition = pendingCursorRef.current;
    pendingCursorRef.current = null;
    textarea.focus();
    textarea.style.height = "auto";
    textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
    textarea.setSelectionRange(cursorPosition, cursorPosition);
  }, [text, textareaRef]);

  useEffect(() => {
    if (!slashState.isOpen) {
      return;
    }

    if (filteredCommands.length === 0 && slashState.selectedIndex !== 0) {
      setSlashState((current) =>
        current.isOpen ? { ...current, selectedIndex: 0 } : current,
      );
      return;
    }

    if (slashState.selectedIndex >= filteredCommands.length) {
      setSlashState((current) =>
        current.isOpen
          ? {
              ...current,
              selectedIndex: Math.max(filteredCommands.length - 1, 0),
            }
          : current,
      );
    }
  }, [filteredCommands.length, slashState.isOpen, slashState.selectedIndex]);

  const closeSlashCommand = useCallback(() => {
    setSlashState((current) =>
      current.isOpen ? createClosedState() : current,
    );
  }, []);

  const detectSlashCommand = useCallback(
    (value: string, cursorPosition: number) => {
      if (!value.startsWith("/")) {
        closeSlashCommand();
        return;
      }

      const match = /^\/(\S*)/.exec(value);
      if (!match) {
        closeSlashCommand();
        return;
      }

      const fullToken = match[0];
      const hasWhitespaceAfterToken = value.length > fullToken.length;
      const cursorInsideFirstToken =
        cursorPosition > 0 && cursorPosition <= fullToken.length;

      if (hasWhitespaceAfterToken || !cursorInsideFirstToken) {
        closeSlashCommand();
        return;
      }

      const query = match[1];
      setSlashState((current) => {
        if (current.isOpen && current.query === query) {
          return current;
        }

        return {
          isOpen: true,
          query,
          selectedIndex: 0,
        };
      });
    },
    [closeSlashCommand],
  );

  const setSlashCommandSelectedIndex = useCallback((index: number) => {
    setSlashState((current) =>
      current.isOpen ? { ...current, selectedIndex: index } : current,
    );
  }, []);

  const navigateSlashCommand = useCallback(
    (direction: "up" | "down") => {
      if (filteredCommands.length === 0) {
        return;
      }

      setSlashState((current) => {
        if (!current.isOpen) {
          return current;
        }

        const delta = direction === "down" ? 1 : -1;
        return {
          ...current,
          selectedIndex:
            (current.selectedIndex + delta + filteredCommands.length) %
            filteredCommands.length,
        };
      });
    },
    [filteredCommands.length],
  );

  const confirmSlashCommand = useCallback((): BuiltinSlashCommand | null => {
    if (!slashState.isOpen || filteredCommands.length === 0) {
      return null;
    }

    return (
      filteredCommands[slashState.selectedIndex] ?? filteredCommands[0] ?? null
    );
  }, [filteredCommands, slashState.isOpen, slashState.selectedIndex]);

  const handleSlashCommandSelect = useCallback(
    (command: BuiltinSlashCommand) => {
      const nextText = `${command.command} `;
      pendingCursorRef.current = nextText.length;
      setText(nextText);
      closeSlashCommand();
    },
    [closeSlashCommand, setText],
  );

  return {
    slashCommandOpen: slashState.isOpen,
    slashCommandSelectedIndex: slashState.selectedIndex,
    filteredSlashCommands: filteredCommands,
    detectSlashCommand,
    closeSlashCommand,
    navigateSlashCommand,
    confirmSlashCommand,
    handleSlashCommandSelect,
    setSlashCommandSelectedIndex,
  };
}
