import { getClient } from "@/shared/api/acpConnection";

export interface ProjectInfo {
  id: string;
  name: string;
  description: string;
  prompt: string;
  icon: string;
  color: string;
  preferredProvider: string | null;
  preferredModel: string | null;
  workingDirs: string[];
  useWorktrees: boolean;
  order: number;
  archivedAt: string | null;
}

// Shape returned by _goose/sources/*. Narrowed to project-type sources here.
interface SourceEntry {
  type: "project";
  name: string;
  description: string;
  content: string;
  directory: string;
  global: boolean;
  properties: Record<string, unknown>;
}

function toProjectInfo(source: SourceEntry): ProjectInfo {
  const p = source.properties ?? {};
  return {
    id: source.name,
    name: (p.title as string) ?? source.name,
    description: source.description,
    prompt: source.content,
    icon: (p.icon as string) ?? "",
    color: (p.color as string) ?? "",
    preferredProvider: (p.preferredProvider as string) ?? null,
    preferredModel: (p.preferredModel as string) ?? null,
    workingDirs: (p.workingDirs as string[]) ?? [],
    useWorktrees: (p.useWorktrees as boolean) ?? false,
    order: (p.order as number) ?? 0,
    archivedAt: (p.archivedAt as string) ?? null,
  };
}

function toProperties(
  info: Omit<ProjectInfo, "id" | "description" | "prompt">,
): Record<string, unknown> {
  const props: Record<string, unknown> = {};
  if (info.name) props.title = info.name;
  if (info.icon) props.icon = info.icon;
  if (info.color) props.color = info.color;
  if (info.preferredProvider) props.preferredProvider = info.preferredProvider;
  if (info.preferredModel) props.preferredModel = info.preferredModel;
  if (info.workingDirs?.length) props.workingDirs = info.workingDirs;
  if (info.useWorktrees) props.useWorktrees = info.useWorktrees;
  if (typeof info.order === "number") props.order = info.order;
  return props;
}

function slugify(name: string): string {
  const slug = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || "project";
}

export async function listProjects(): Promise<ProjectInfo[]> {
  const client = await getClient();
  const raw = await client.extMethod("_goose/sources/list", {
    type: "project",
  });
  const sources = (raw.sources ?? []) as SourceEntry[];
  return sources
    .map(toProjectInfo)
    .filter((p) => p.archivedAt === null)
    .sort((a, b) => a.order - b.order);
}

export async function createProject(
  name: string,
  description: string,
  prompt: string,
  icon: string,
  color: string,
  preferredProvider: string | null,
  preferredModel: string | null,
  workingDirs: string[],
  useWorktrees: boolean,
): Promise<ProjectInfo> {
  const client = await getClient();
  const id = slugify(name);
  const raw = await client.extMethod("_goose/sources/create", {
    type: "project",
    name: id,
    description,
    content: prompt,
    global: true,
    properties: toProperties({
      name,
      icon,
      color,
      preferredProvider,
      preferredModel,
      workingDirs,
      useWorktrees,
      order: 0,
      archivedAt: null,
    }),
  });
  return toProjectInfo(raw.source as SourceEntry);
}

export async function updateProject(
  existing: ProjectInfo,
  updates: Partial<Omit<ProjectInfo, "id">>,
): Promise<ProjectInfo> {
  const merged = { ...existing, ...updates };
  const client = await getClient();
  const raw = await client.extMethod("_goose/sources/update", {
    type: "project",
    name: existing.id,
    description: merged.description,
    content: merged.prompt,
    global: true,
    properties: toProperties({
      name: merged.name,
      icon: merged.icon,
      color: merged.color,
      preferredProvider: merged.preferredProvider,
      preferredModel: merged.preferredModel,
      workingDirs: merged.workingDirs,
      useWorktrees: merged.useWorktrees,
      order: merged.order,
      archivedAt: merged.archivedAt,
    }),
  });
  return toProjectInfo(raw.source as SourceEntry);
}

export async function deleteProject(id: string): Promise<void> {
  const client = await getClient();
  await client.extMethod("_goose/sources/delete", {
    type: "project",
    name: id,
    global: true,
  });
}

export async function getProject(id: string): Promise<ProjectInfo> {
  const client = await getClient();
  const raw = await client.extMethod("_goose/sources/list", {
    type: "project",
  });
  const sources = (raw.sources ?? []) as SourceEntry[];
  const match = sources.find((s) => s.name === id);
  if (!match) throw new Error(`Project "${id}" not found`);
  return toProjectInfo(match);
}

export async function archiveProject(id: string): Promise<void> {
  // Read current, update with archivedAt property
  const client = await getClient();
  const raw = await client.extMethod("_goose/sources/list", {
    type: "project",
  });
  const sources = (raw.sources ?? []) as SourceEntry[];
  const existing = sources.find((s) => s.name === id);
  if (!existing) throw new Error(`Project "${id}" not found`);

  const props = { ...(existing.properties ?? {}) };
  props.archivedAt = new Date().toISOString();

  await client.extMethod("_goose/sources/update", {
    type: "project",
    name: id,
    description: existing.description,
    content: existing.content,
    global: true,
    properties: props,
  });
}

export async function restoreProject(id: string): Promise<void> {
  const client = await getClient();
  const raw = await client.extMethod("_goose/sources/list", {
    type: "project",
  });
  const sources = (raw.sources ?? []) as SourceEntry[];
  const existing = sources.find((s) => s.name === id);
  if (!existing) throw new Error(`Project "${id}" not found`);

  const props = { ...(existing.properties ?? {}) };
  delete props.archivedAt;

  await client.extMethod("_goose/sources/update", {
    type: "project",
    name: id,
    description: existing.description,
    content: existing.content,
    global: true,
    properties: props,
  });
}

export async function reorderProjects(
  order: [string, number][],
): Promise<void> {
  const client = await getClient();
  // Update each project's order property
  for (const [id, orderValue] of order) {
    const raw = await client.extMethod("_goose/sources/list", {
      type: "project",
    });
    const sources = (raw.sources ?? []) as SourceEntry[];
    const existing = sources.find((s) => s.name === id);
    if (!existing) continue;

    const props = { ...(existing.properties ?? {}), order: orderValue };
    await client.extMethod("_goose/sources/update", {
      type: "project",
      name: id,
      description: existing.description,
      content: existing.content,
      global: true,
      properties: props,
    });
  }
}

export async function listArchivedProjects(): Promise<ProjectInfo[]> {
  // List all, filter for archived
  const all = await listProjects();
  return all.filter((p) => p.archivedAt !== null);
}
