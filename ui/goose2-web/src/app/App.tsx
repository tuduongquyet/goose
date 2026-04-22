import { useEffect } from "react";

import { AppShell } from "@/app/AppShell";
import { useScrollFade } from "@/shared/hooks/useScrollFade";
import { useZoom } from "@/shared/hooks/useZoom";
import { Toaster } from "@/shared/ui/sonner";

export function App() {
  useScrollFade();
  useZoom();
  useEffect(() => {
    const preventWindowFileNavigation = (event: DragEvent) => {
      event.preventDefault();
    };

    window.addEventListener("dragover", preventWindowFileNavigation);
    window.addEventListener("drop", preventWindowFileNavigation);

    return () => {
      window.removeEventListener("dragover", preventWindowFileNavigation);
      window.removeEventListener("drop", preventWindowFileNavigation);
    };
  }, []);

  return (
    <>
      <AppShell />
      <Toaster />
    </>
  );
}
