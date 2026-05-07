import { getClient } from "@/shared/api/acpConnection";

export function connectToServer(): void {
  // Eagerly start ACP connection (non-blocking)
  getClient().catch(() => {
    // Connection will be retried on next use
  });
}
