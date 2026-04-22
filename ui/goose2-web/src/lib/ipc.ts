/**
 * IPC compatibility layer — drop-in replacement for @tauri-apps/api/core
 * and @tauri-apps/api/event so existing code needs only an import-path change.
 */

export { listen } from "./event-bus";
export type { } from "./event-bus";

// Re-export a type alias so `UnlistenFn` works as before.
export type UnlistenFn = () => void;

export function generateId(): string {
  return crypto.randomUUID();
}

// -----------------------------------------------------------------------
// Local storage helpers for non-ACP data
// -----------------------------------------------------------------------

function getStorage<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? JSON.parse(raw) : fallback;
  } catch {
    return fallback;
  }
}

function setStorage(key: string, value: unknown): void {
  localStorage.setItem(key, JSON.stringify(value));
}

// -----------------------------------------------------------------------
// invoke() — maps Tauri command names to real calls or local stubs
// -----------------------------------------------------------------------

export async function invoke<T = unknown>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  switch (command) {
    // ----- Server URL -----

    case "get_goose_serve_url": {
      // __GOOSE_SERVER_URL__ is injected by vite.config.ts at build time
      const serverUrl = (typeof (globalThis as Record<string, unknown>).__GOOSE_SERVER_URL__ !== "undefined"
        ? (globalThis as Record<string, unknown>).__GOOSE_SERVER_URL__ as string
        : null) ?? `http://${window.location.hostname}:3284`;
      return serverUrl as T;
    }

    // ----- Projects (localStorage) -----

    case "list_projects":
      return getStorage("goose_projects", []) as T;

    case "create_project": {
      const projects = getStorage<Record<string, unknown>[]>("goose_projects", []);
      const project = {
        id: generateId(),
        name: args!.name,
        description: args!.description ?? "",
        prompt: args!.prompt ?? "",
        icon: args!.icon ?? "",
        color: args!.color ?? "#64748b",
        preferredProvider: args!.preferredProvider ?? null,
        preferredModel: args!.preferredModel ?? null,
        workingDirs: args!.workingDirs ?? [],
        useWorktrees: args!.useWorktrees ?? false,
        order: projects.length,
        archivedAt: null,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        artifactsDir: "",
      };
      projects.push(project);
      setStorage("goose_projects", projects);
      return project as T;
    }

    case "update_project": {
      const projects = getStorage<Record<string, unknown>[]>("goose_projects", []);
      const idx = projects.findIndex((p) => p.id === args!.id);
      if (idx >= 0) {
        projects[idx] = { ...projects[idx], ...args, updatedAt: new Date().toISOString() };
        setStorage("goose_projects", projects);
        return projects[idx] as T;
      }
      throw new Error("Project not found");
    }

    case "delete_project": {
      const projects = getStorage<Record<string, unknown>[]>("goose_projects", []);
      setStorage("goose_projects", projects.filter((p) => p.id !== args!.id));
      return undefined as T;
    }

    case "get_project": {
      const projects = getStorage<Record<string, unknown>[]>("goose_projects", []);
      const found = projects.find((p) => p.id === args!.id);
      if (!found) throw new Error("Project not found");
      return found as T;
    }

    case "list_archived_projects":
      return getStorage<Record<string, unknown>[]>("goose_projects", []).filter(
        (p) => p.archivedAt,
      ) as T;

    case "archive_project": {
      const projects = getStorage<Record<string, unknown>[]>("goose_projects", []);
      const idx = projects.findIndex((p) => p.id === args!.id);
      if (idx >= 0) {
        projects[idx].archivedAt = new Date().toISOString();
        setStorage("goose_projects", projects);
      }
      return undefined as T;
    }

    case "restore_project": {
      const projects = getStorage<Record<string, unknown>[]>("goose_projects", []);
      const idx = projects.findIndex((p) => p.id === args!.id);
      if (idx >= 0) {
        projects[idx].archivedAt = null;
        setStorage("goose_projects", projects);
      }
      return undefined as T;
    }

    // ----- Personas (stubs — no server-side support) -----

    case "list_personas":
      return getStorage("goose_personas", []) as T;

    case "create_persona": {
      const personas = getStorage<Record<string, unknown>[]>("goose_personas", []);
      const req = args!.request as Record<string, unknown>;
      const persona = { id: generateId(), ...req, createdAt: new Date().toISOString() };
      personas.push(persona);
      setStorage("goose_personas", personas);
      return persona as T;
    }

    case "update_persona": {
      const personas = getStorage<Record<string, unknown>[]>("goose_personas", []);
      const idx = personas.findIndex((p) => p.id === args!.id);
      if (idx >= 0) {
        personas[idx] = { ...personas[idx], ...(args!.request as Record<string, unknown>) };
        setStorage("goose_personas", personas);
        return personas[idx] as T;
      }
      throw new Error("Persona not found");
    }

    case "delete_persona": {
      const personas = getStorage<Record<string, unknown>[]>("goose_personas", []);
      setStorage("goose_personas", personas.filter((p) => p.id !== args!.id));
      return undefined as T;
    }

    case "refresh_personas":
      return getStorage("goose_personas", []) as T;

    case "export_persona":
      return { json: "{}", suggestedFilename: "persona.json" } as T;

    case "import_personas":
      return [] as T;

    case "save_persona_avatar":
    case "save_persona_avatar_bytes":
      return "" as T;

    case "get_avatars_dir":
      return "" as T;

    // ----- Skills (stubs) -----

    case "list_skills":
      return getStorage("goose_skills", []) as T;

    case "create_skill": {
      const skills = getStorage<Record<string, unknown>[]>("goose_skills", []);
      const skill = {
        name: args!.name,
        description: args!.description,
        instructions: args!.instructions,
        path: "",
      };
      skills.push(skill);
      setStorage("goose_skills", skills);
      return undefined as T;
    }

    case "delete_skill": {
      const skills = getStorage<Record<string, unknown>[]>("goose_skills", []);
      setStorage("goose_skills", skills.filter((s) => s.name !== args!.name));
      return undefined as T;
    }

    case "update_skill": {
      const skills = getStorage<Record<string, unknown>[]>("goose_skills", []);
      const idx = skills.findIndex((s) => s.name === args!.name);
      if (idx >= 0) {
        skills[idx] = { ...skills[idx], ...args };
        setStorage("goose_skills", skills);
        return skills[idx] as T;
      }
      throw new Error("Skill not found");
    }

    case "export_skill":
      return { json: "{}", filename: "skill.json" } as T;

    case "import_skills":
      return [] as T;

    // ----- Credentials / Providers (stubs) -----

    case "get_provider_config":
      return [] as T;

    case "save_provider_field":
      return undefined as T;

    case "delete_provider_config":
      return undefined as T;

    case "check_all_provider_status":
      return [] as T;

    case "restart_app":
      window.location.reload();
      return undefined as T;

    // ----- Model/Agent setup (stubs) -----

    case "authenticate_model_provider":
      return undefined as T;

    case "check_agent_installed":
      return true as T;

    case "check_agent_auth":
      return true as T;

    case "install_agent":
      return undefined as T;

    case "authenticate_agent":
      return undefined as T;

    // ----- System (web-compatible stubs) -----

    case "resolve_path": {
      const parts = (args!.request as { parts: string[] }).parts;
      // Join path parts, collapse duplicate slashes, but keep leading ~/
      const joined = parts.join("/").replace(/\/+/g, "/");
      return { path: joined } as T;
    }

    case "get_home_dir":
      return "/home/user" as T;

    case "save_exported_session_file": {
      const blob = new Blob([args!.contents as string], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = (args!.defaultFilename as string) || "session.json";
      a.click();
      URL.revokeObjectURL(url);
      return (args!.defaultFilename as string) as T;
    }

    case "path_exists":
      return false as T;

    case "list_files_for_mentions":
      return [] as T;

    case "list_directory_entries":
      return [] as T;

    // ----- Git (stubs — can't access server filesystem from browser) -----

    case "get_git_state":
      return { currentBranch: null, branches: [], remotes: [], isRepo: false } as T;

    case "git_switch_branch":
    case "git_stash":
    case "git_init":
    case "git_fetch":
    case "git_pull":
    case "git_create_branch":
    case "git_create_worktree":
      return undefined as T;

    case "get_changed_files":
      return [] as T;

    // ----- Doctor (stubs) -----

    case "run_doctor":
      return { checks: [] } as T;

    case "run_doctor_fix":
      return undefined as T;

    // ----- Fallback -----

    default:
      console.warn(`[ipc] unhandled command: ${command}`, args);
      return undefined as T;
  }
}

/**
 * No-op replacement for `convertFileSrc()` from @tauri-apps/api/core.
 * In the web version, local file paths can't be converted to URLs.
 */
export function convertFileSrc(filePath: string, _protocol?: string): string {
  return filePath;
}
