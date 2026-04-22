import type { ProjectInfo } from "../api/projects";
import { resolvePath } from "@/shared/api/pathResolver";

export interface ProjectFolderOption {
  id: string;
  name: string;
  path?: string;
}

function trimValue(value: string | null | undefined): string | null {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

export function getProjectFolderName(path: string): string {
  const normalized = path.replace(/[\\/]+$/, "");
  if (!normalized) {
    return path;
  }

  const parts = normalized.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? normalized;
}

function appendArtifactsSegment(path: string): string {
  return `${path.replace(/[\\/]+$/, "")}/artifacts`;
}

function resolveProjectArtifactRoots(
  project: Pick<ProjectInfo, "workingDirs"> | null | undefined,
): string[] {
  const workingDirs = (project?.workingDirs ?? [])
    .map((directory) => trimValue(directory))
    .filter((directory): directory is string => directory !== null);

  return workingDirs.map(appendArtifactsSegment);
}

export function getProjectArtifactRoots(
  project: Pick<ProjectInfo, "workingDirs"> | null | undefined,
): string[] {
  return resolveProjectArtifactRoots(project);
}

export function resolveProjectDefaultArtifactRoot(
  project: Pick<ProjectInfo, "workingDirs"> | null | undefined,
): string | undefined {
  const workingDirs = (project?.workingDirs ?? [])
    .map((directory) => trimValue(directory))
    .filter((directory): directory is string => directory !== null);

  if (workingDirs.length > 0) {
    return appendArtifactsSegment(workingDirs[0]);
  }

  return undefined;
}

export async function defaultGlobalArtifactRoot(): Promise<string> {
  return (await resolvePath({ parts: ["~", ".goose", "artifacts"] })).path;
}

export function getProjectFolderOption(
  project: Pick<ProjectInfo, "workingDirs"> | null | undefined,
): ProjectFolderOption[] {
  return resolveProjectArtifactRoots(project).map((d) => ({
    id: d,
    name: getProjectFolderName(d),
    path: d,
  }));
}

export function composeSystemPrompt(
  ...parts: Array<string | null | undefined>
): string | undefined {
  const combined = parts
    .map((part) => trimValue(part))
    .filter((part): part is string => part !== null);

  return combined.length > 0 ? combined.join("\n\n") : undefined;
}
