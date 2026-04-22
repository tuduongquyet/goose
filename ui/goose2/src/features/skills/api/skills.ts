import { getClient } from "@/shared/api/acpConnection";

export interface SkillInfo {
  name: string;
  description: string;
  instructions: string;
  path: string;
  global: boolean;
  /** Present when the skill belongs to a project (non-global). */
  projectName?: string;
  /** The project root directory. Needed for update/delete of non-global skills. */
  projectDir?: string;
}

// Shape returned by _goose/sources/*. Narrowed to skill-type sources here.
interface SourceEntry {
  type: "skill";
  name: string;
  description: string;
  content: string;
  directory: string;
  global: boolean;
  properties?: Record<string, unknown>;
}

function toSkillInfo(source: SourceEntry): SkillInfo {
  const info: SkillInfo = {
    name: source.name,
    description: source.description,
    instructions: source.content,
    path: source.directory,
    global: source.global,
  };
  const projectName = source.properties?.projectName;
  if (typeof projectName === "string") {
    info.projectName = projectName;
  }
  const projectDir = source.properties?.projectDir;
  if (typeof projectDir === "string") {
    info.projectDir = projectDir;
  }
  return info;
}

export async function createSkill(
  name: string,
  description: string,
  instructions: string,
  options?: { projectId?: string },
): Promise<void> {
  const client = await getClient();
  await client.extMethod("_goose/sources/create", {
    type: "skill",
    name,
    description,
    content: instructions,
    global: !options?.projectId,
    ...(options?.projectId ? { projectId: options.projectId } : {}),
  });
}

export async function listSkills(options?: {
  includeProjectSources?: boolean;
}): Promise<SkillInfo[]> {
  const client = await getClient();
  const raw = await client.extMethod("_goose/sources/list", {
    type: "skill",
    includeProjectSources: options?.includeProjectSources ?? true,
  });
  const sources = (raw.sources ?? []) as SourceEntry[];
  return sources.map(toSkillInfo);
}

export async function deleteSkill(
  name: string,
  options?: { projectDir?: string },
): Promise<void> {
  const client = await getClient();
  await client.extMethod("_goose/sources/delete", {
    type: "skill",
    name,
    global: !options?.projectDir,
    ...(options?.projectDir ? { projectDir: options.projectDir } : {}),
  });
}

export async function updateSkill(
  name: string,
  description: string,
  instructions: string,
  options?: { projectDir?: string },
): Promise<SkillInfo> {
  const client = await getClient();
  const raw = await client.extMethod("_goose/sources/update", {
    type: "skill",
    name,
    description,
    content: instructions,
    global: !options?.projectDir,
    ...(options?.projectDir ? { projectDir: options.projectDir } : {}),
  });
  return toSkillInfo(raw.source as SourceEntry);
}

export async function exportSkill(
  name: string,
  options?: { projectDir?: string },
): Promise<{ json: string; filename: string }> {
  const client = await getClient();
  const raw = await client.extMethod("_goose/sources/export", {
    type: "skill",
    name,
    global: !options?.projectDir,
    ...(options?.projectDir ? { projectDir: options.projectDir } : {}),
  });
  return { json: raw.json as string, filename: raw.filename as string };
}

export async function importSkills(
  fileBytes: number[],
  fileName: string,
  options?: { projectId?: string },
): Promise<SkillInfo[]> {
  if (!fileName.endsWith(".skill.json") && !fileName.endsWith(".json")) {
    throw new Error("File must have a .skill.json or .json extension");
  }
  const data = new TextDecoder().decode(new Uint8Array(fileBytes));
  const client = await getClient();
  const raw = await client.extMethod("_goose/sources/import", {
    data,
    global: !options?.projectId,
    ...(options?.projectId ? { projectId: options.projectId } : {}),
  });
  const sources = (raw.sources ?? []) as SourceEntry[];
  return sources.map(toSkillInfo);
}
