import type {
  ProviderInventoryEntryDto,
  RefreshProviderInventoryResponse,
} from "@aaif/goose-sdk";
import { getClient } from "@/shared/api/acpConnection";
import { perfLog } from "@/shared/lib/perfLog";

export async function getProviderInventory(
  providerIds: string[] = [],
): Promise<ProviderInventoryEntryDto[]> {
  const client = await getClient();
  const t0 = performance.now();
  try {
    const response = await client.goose.GooseProvidersList({ providerIds });
    const entries = response.entries ?? [];
    perfLog(
      `[perf:inventory] getProviderInventory done in ${(performance.now() - t0).toFixed(1)}ms (n=${entries.length})`,
    );
    return entries;
  } catch {
    // Server may not support this goose-specific extension
    return [];
  }
}

export async function refreshProviderInventory(
  providerIds: string[] = [],
): Promise<RefreshProviderInventoryResponse> {
  const client = await getClient();
  const t0 = performance.now();
  const response = await client.goose.GooseProvidersInventoryRefresh({
    providerIds,
  });
  perfLog(
    `[perf:inventory] refreshProviderInventory done in ${(performance.now() - t0).toFixed(1)}ms started=[${response.started.join(",")}]`,
  );
  return response;
}
