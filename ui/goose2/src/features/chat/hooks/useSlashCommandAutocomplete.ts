import { useCallback, useEffect, useMemo, useState } from "react";
import {
  filterBuiltinSlashCommands,
  type BuiltinSlashCommand,
} from "../lib/slashCommands";

function createClosedState() {
  return {
    isOpen: false,
    query: "",
    selectedIndex: 0,
  };
}

export function useSlashCommandAutocomplete() {
  const [slashState, setSlashState] = useState(createClosedState);

  const filteredCommands = useMemo(
    () => filterBuiltinSlashCommands(slashState.query),
    [slashState.query],
  );

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

  return {
    slashCommandOpen: slashState.isOpen,
    slashCommandSelectedIndex: slashState.selectedIndex,
    filteredSlashCommands: filteredCommands,
    detectSlashCommand,
    closeSlashCommand,
    navigateSlashCommand,
    confirmSlashCommand,
    setSlashCommandSelectedIndex,
  };
}
