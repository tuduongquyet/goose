import { Folder } from "lucide-react";
import { cn } from "@/shared/lib/cn";

interface ProjectGlyphProps {
  color?: string | null;
  className?: string;
}

export function ProjectGlyph({ color, className }: ProjectGlyphProps) {
  return (
    <Folder
      aria-hidden="true"
      strokeWidth={2}
      className={cn(
        "shrink-0",
        color ? "" : "text-muted-foreground/50",
        className,
      )}
      style={color ? { color } : undefined}
    />
  );
}
