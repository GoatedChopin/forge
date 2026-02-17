<script lang="ts">
  import { goto } from "$app/navigation";
  import { getForgeClient, listProjects$, createProject, updateProject } from "$lib/forge";
  import type { ForgeError, Project } from "$lib/forge";
  import { getUserId } from "$lib/auth";

  const userId = getUserId();
  const projects = listProjects$({ user_id: userId });
  let localProjects: Project[] = $state([]);

  let newName = $state("");
  let creating = $state(false);
  let error: string | null = $state(null);
  let visibleProjects: Project[] = $derived.by(() => {
    const map = new Map<string, Project>();
    for (const project of projects.data ?? []) {
      map.set(project.id, project);
    }
    for (const project of localProjects) {
      map.set(project.id, project);
    }
    return Array.from(map.values());
  });

  async function handleCreate() {
    if (!newName.trim()) return;
    creating = true;
    error = null;
    try {
      const created = await createProject({
        user_id: userId,
        name: newName.trim(),
        description: "",
      });
      localProjects = [created, ...localProjects];
      newName = "";
    } catch (e) {
      error = (e as ForgeError).message;
    } finally {
      creating = false;
    }
  }

  async function handleRename(project: { id: string; name: string }) {
    const next = prompt("Rename project", project.name);
    if (!next) return;

    try {
      const updated = await updateProject({
        user_id: userId,
        id: project.id,
        name: next.trim(),
      });
      localProjects = [updated, ...localProjects.filter((p) => p.id !== updated.id)];
    } catch (e) {
      error = (e as ForgeError).message;
    }
  }

  async function handleLogout() {
    localStorage.removeItem("trellix_token");
    localStorage.removeItem("trellix_user");
    await getForgeClient().reconnect();
    goto("/");
  }
</script>

<main>
  <header>
    <h1>Projects</h1>
    <button class="logout" onclick={handleLogout}>Sign out</button>
  </header>

  <form
    class="create-form"
    onsubmit={(event) => {
      event.preventDefault();
      void handleCreate();
    }}
  >
    <input
      bind:value={newName}
      placeholder="New project name"
      disabled={creating}
    />
    <button type="submit" disabled={creating || !newName.trim()}>
      {creating ? "..." : "Create"}
    </button>
  </form>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if projects.loading}
    <p class="status">Loading...</p>
  {:else if projects.error}
    <p class="error">{projects.error.message}</p>
  {:else if projects.data}
    {#if visibleProjects.length === 0}
      <p class="status">No projects yet. Create one above.</p>
    {:else}
      <ul class="project-list">
        {#each visibleProjects as project (project.id)}
          <li>
            <div class="project-row">
              <a href="/app/{project.id}">
                <span class="name">{project.name}</span>
                {#if project.archive_delete_at}
                  <span class="badge archived">scheduled</span>
                {:else if project.archived}
                  <span class="badge archived">completed</span>
                {/if}
                {#if project.description}
                  <span class="desc">{project.description}</span>
                {/if}
              </a>
              <button
                class="rename"
                onclick={() => handleRename(project)}
                disabled={project.archived}
              >
                Rename
              </button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</main>

<style>
  main {
    max-width: 600px;
    margin: 2rem auto;
    padding: 0 1rem;
    font-family: system-ui, sans-serif;
  }

  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1.5rem;
  }

  h1 {
    margin: 0;
  }

  .logout {
    background: none;
    border: 1px solid #ccc;
    padding: 0.375rem 0.75rem;
    border-radius: 4px;
    cursor: pointer;
    font-size: 0.875rem;
  }

  .create-form {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 1.5rem;
  }

  .create-form input {
    flex: 1;
    padding: 0.5rem;
    border: 1px solid #ccc;
    border-radius: 4px;
    font-size: 1rem;
  }

  .create-form button {
    padding: 0.5rem 1rem;
    background: #2563eb;
    color: white;
    border: none;
    border-radius: 4px;
    cursor: pointer;
  }

  .create-form button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .project-list {
    list-style: none;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .project-list li a {
    display: block;
    flex: 1;
    padding: 1rem;
    border: 1px solid #ddd;
    border-radius: 8px;
    text-decoration: none;
    color: inherit;
  }

  .project-list li a:hover {
    border-color: #2563eb;
    background: #f8faff;
  }

  .project-row {
    display: flex;
    gap: 0.5rem;
    align-items: stretch;
  }

  .rename {
    border: 1px solid #ccc;
    background: white;
    border-radius: 8px;
    padding: 0 0.75rem;
    cursor: pointer;
    font-size: 0.875rem;
  }

  .rename:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .name {
    font-weight: 600;
    font-size: 1.1rem;
  }

  .desc {
    display: block;
    color: #666;
    font-size: 0.875rem;
    margin-top: 0.25rem;
  }

  .badge {
    font-size: 0.75rem;
    padding: 0.125rem 0.5rem;
    border-radius: 9999px;
    margin-left: 0.5rem;
  }

  .badge.archived {
    background: #fef3c7;
    color: #92400e;
  }

  .error {
    color: #dc2626;
    font-size: 0.875rem;
    padding: 0.5rem;
    background: #fef2f2;
    border-radius: 4px;
  }

  .status {
    color: #666;
    text-align: center;
    padding: 2rem;
  }
</style>
