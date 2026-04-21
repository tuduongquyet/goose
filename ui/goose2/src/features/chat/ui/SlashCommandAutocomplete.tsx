import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/shared/lib/cn";
import { PopoverContent } from "@/shared/ui/popover";
import type { BuiltinSlashCommand } from "../lib/slashCommands";

interface SlashCommandAutocompleteProps {
  commands: BuiltinSlashCommand[];
  isOpen: boolean;
  onSelect: (command: BuiltinSlashCommand) => void;
  onHighlightIndex?: (index: number) => void;
  selectedIndex?: number;
}

export function SlashCommandAutocomplete({
  commands,
  isOpen,
  onSelect,
  onHighlightIndex,
  selectedIndex = 0,
}: SlashCommandAutocompleteProps) {
  const { t } = useTranslation("chat");
  const itemRefs = useRef<Map<number, HTMLButtonElement>>(new Map());

  useEffect(() => {
    const element = itemRefs.current.get(selectedIndex);
    if (element) {
      element.scrollIntoView({ block: "nearest" });
    }
  }, [selectedIndex]);

  if (!isOpen) {
    return null;
  }

  return (
    <PopoverContent
      side="top"
      align="start"
      sideOffset={4}
      className="w-80 px-1 py-1"
      onOpenAutoFocus={(event) => event.preventDefault()}
      onCloseAutoFocus={(event) => event.preventDefault()}
      onEscapeKeyDown={(event) => event.preventDefault()}
      onInteractOutside={(event) => event.preventDefault()}
      role="listbox"
      aria-label={t("slashCommands.ariaLabel")}
    >
      <div className="max-h-56 overflow-y-auto">
        <div className="px-2 py-1 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          {t("slashCommands.title")}
        </div>

        {commands.length === 0 ? (
          <div className="px-2 py-2 text-sm text-muted-foreground">
            {t("slashCommands.empty")}
          </div>
        ) : (
          commands.map((command, index) => (
            <button
              key={command.name}
              ref={(element) => {
                if (element) {
                  itemRefs.current.set(index, element);
                } else {
                  itemRefs.current.delete(index);
                }
              }}
              type="button"
              role="option"
              aria-selected={index === selectedIndex}
              className={cn(
                "flex w-full flex-col gap-1 rounded-md px-2 py-2 text-left transition-colors",
                index === selectedIndex
                  ? "bg-accent text-foreground"
                  : "text-muted-foreground hover:bg-accent/50",
              )}
              onClick={() => onSelect(command)}
              onMouseEnter={() => onHighlightIndex?.(index)}
            >
              <div className="flex items-center gap-2">
                <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-[11px] text-foreground">
                  {command.command}
                </span>
                <span className="text-sm font-medium text-foreground">
                  {command.label}
                </span>
              </div>
              <span className="text-xs text-muted-foreground">
                {command.description}
              </span>
            </button>
          ))
        )}
      </div>
    </PopoverContent>
  );
}
