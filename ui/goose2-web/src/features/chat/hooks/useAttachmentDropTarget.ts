import {
  useCallback,
  useRef,
  useState,
  type DragEvent,
  type RefObject,
} from "react";

interface UseAttachmentDropTargetOptions {
  disabled: boolean;
  isStreaming: boolean;
  targetRef: RefObject<HTMLDivElement | null>;
  onDropFiles: (files: File[]) => void;
  onDropPaths: (paths: string[]) => void;
}

function hasDraggedFiles(dataTransfer: DataTransfer) {
  return (
    Array.from(dataTransfer.items).some((item) => item.kind === "file") ||
    Array.from(dataTransfer.types).includes("Files")
  );
}

export function useAttachmentDropTarget({
  disabled,
  isStreaming,
  onDropFiles,
}: UseAttachmentDropTargetOptions) {
  const [isAttachmentDragOver, setIsAttachmentDragOver] = useState(false);
  const dragDepthRef = useRef(0);

  const handleDragEnter = useCallback(
    (event: DragEvent<HTMLDivElement>) => {
      const draggedFiles = hasDraggedFiles(event.dataTransfer);
      if (disabled || isStreaming || !draggedFiles) {
        return;
      }

      event.preventDefault();
      dragDepthRef.current += 1;
      setIsAttachmentDragOver(true);
    },
    [disabled, isStreaming],
  );

  const handleDragOver = useCallback(
    (event: DragEvent<HTMLDivElement>) => {
      const draggedFiles = hasDraggedFiles(event.dataTransfer);
      if (disabled || isStreaming || !draggedFiles) {
        return;
      }

      event.preventDefault();
      event.dataTransfer.dropEffect = "copy";
      setIsAttachmentDragOver(true);
    },
    [disabled, isStreaming],
  );

  const handleDragLeave = useCallback((event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    dragDepthRef.current = Math.max(0, dragDepthRef.current - 1);
    if (dragDepthRef.current === 0) {
      setIsAttachmentDragOver(false);
    }
  }, []);

  const handleDrop = useCallback(
    (event: DragEvent<HTMLDivElement>) => {
      const draggedFiles = hasDraggedFiles(event.dataTransfer);
      if (disabled || isStreaming || !draggedFiles) {
        return;
      }

      event.preventDefault();
      dragDepthRef.current = 0;
      setIsAttachmentDragOver(false);

      const files = Array.from(event.dataTransfer.files);
      if (files.length === 0) {
        return;
      }

      onDropFiles(files);
    },
    [disabled, isStreaming, onDropFiles],
  );

  return {
    isAttachmentDragOver,
    handleDragEnter,
    handleDragOver,
    handleDragLeave,
    handleDrop,
  };
}
