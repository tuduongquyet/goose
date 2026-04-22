import { invoke } from "@/lib/ipc";

export interface ResolvePathParams {
  parts: string[];
}

export interface ResolvedPath {
  path: string;
}

export async function resolvePath({
  parts,
}: ResolvePathParams): Promise<ResolvedPath> {
  return invoke("resolve_path", {
    request: { parts },
  });
}
