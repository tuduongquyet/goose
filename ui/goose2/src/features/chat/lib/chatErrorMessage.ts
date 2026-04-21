export function getChatErrorMessage(error: unknown): string {
  // Tauri command rejections typically arrive as plain strings, so handle
  // that shape first before falling back to standard Error objects.
  if (typeof error === "string" && error.trim()) {
    return error;
  }

  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }

  if (
    typeof error === "object" &&
    error !== null &&
    "message" in error &&
    typeof error.message === "string" &&
    error.message.trim()
  ) {
    return error.message;
  }

  return "Unknown error";
}
