<script lang="ts">
  import { goto } from "$app/navigation";
  import { resolve } from "$app/paths";
  import { SvelteMap } from "svelte/reactivity";
  import {
    auth,
    listProjects$,
    createProject,
    updateProject,
  } from "$lib/forge";
  import type { ForgeError, Project } from "$lib/forge";
  import { getUserId } from "$lib/auth";

  const userId = getUserId();
  const projects = listProjects$({ user_id: userId });
  let localProjects: Project[] = $state([]);

  let newName = $state("");
  let creating = $state(false);
  let error: string | null = $state(null);
  let renamingId: string | null = $state(null);
  let renameValue = $state("");
  let visibleProjects: Project[] = $derived.by(() => {
    const map = new SvelteMap<string, Project>();
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

  function startRename(project: { id: string; name: string }) {
    renamingId = project.id;
    renameValue = project.name;
  }

  async function submitRename(projectId: string) {
    if (!renameValue.trim()) {
      renamingId = null;
      return;
    }
    try {
      const updated = await updateProject({
        user_id: userId,
        id: projectId,
        name: renameValue.trim(),
      });
      localProjects = [
        updated,
        ...localProjects.filter((p) => p.id !== updated.id),
      ];
    } catch (e) {
      error = (e as ForgeError).message;
    } finally {
      renamingId = null;
    }
  }

  function handleRenameKeydown(event: KeyboardEvent, projectId: string) {
    if (event.key === "Enter") {
      void submitRename(projectId);
    } else if (event.key === "Escape") {
      renamingId = null;
    }
  }

  async function handleLogout() {
    auth.clearAuth();
    goto(resolve("/"));
  }
</script>

<main>
  <div class="shell">
    <header>
      <div>
        <h1>Projects</h1>
        <p class="subtitle">
          {visibleProjects.length} project{visibleProjects.length !== 1
            ? "s"
            : ""}
        </p>
      </div>
      <button class="logout" onclick={handleLogout}>Sign out</button>
    </header>

    <section class="create-section">
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
    </section>

    {#if error}
      <p class="error">{error}</p>
    {/if}

    {#if projects.loading}
      <p class="status">Loading...</p>
    {:else if projects.error}
      <p class="error">{projects.error.message}</p>
    {:else if projects.data}
      {#if visibleProjects.length === 0}
        <section class="empty-panel">
          <p class="status">No projects yet. Create one above.</p>
        </section>
      {:else}
        <ul class="project-list">
          {#each visibleProjects as project (project.id)}
            <li>
              <div class="project-row">
                {#if renamingId === project.id}
                  <div class="rename-form">
                    <input
                      class="rename-input"
                      bind:value={renameValue}
                      onkeydown={(e) => handleRenameKeydown(e, project.id)}
                      onblur={() => submitRename(project.id)}
                    />
                  </div>
                {:else}
                  <a href={resolve(`/app/${project.id}`)}>
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
                    onclick={() => startRename(project)}
                    disabled={project.archived}
                  >
                    Rename
                  </button>
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  </div>
</main>

<style>
  :global(body) {
    margin: 0;
    background: #fff;
    color: #222;
    font-family:
      -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    min-height: 100vh;
  }

  main {
    max-width: 600px;
    margin: 0 auto;
    padding: 0 1rem;
  }

  .shell {
    padding: 2rem 0 3rem;
    display: grid;
    gap: 1rem;
  }

  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  h1 {
    margin: 0;
    font-weight: 600;
    font-size: 1.5rem;
    color: #111;
  }

  .subtitle {
    margin: 0.15rem 0 0;
    font-size: 0.82rem;
    color: #888;
  }

  .logout {
    background: none;
    border: 1px solid #ccc;
    padding: 0.35rem 0.75rem;
    border-radius: 4px;
    cursor: pointer;
    font-size: 0.82rem;
    color: #555;
    font-family: inherit;
  }

  .logout:hover {
    border-color: #999;
    color: #222;
  }

  .create-section {
    border: 1px solid #e5e5e5;
    border-radius: 6px;
    padding: 0.75rem;
  }

  .create-form {
    display: flex;
    gap: 0.5rem;
  }

  .create-form input {
    flex: 1;
    padding: 0.5rem 0.75rem;
    border: 1px solid #ccc;
    border-radius: 4px;
    background: #fff;
    color: #222;
    font: inherit;
    font-size: 0.9rem;
    outline: none;
  }

  .create-form input:focus {
    border-color: #888;
  }

  .create-form button {
    padding: 0.5rem 1rem;
    background: #111;
    color: #fff;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-weight: 500;
    font-family: inherit;
    font-size: 0.9rem;
  }

  .create-form button:hover {
    background: #333;
  }

  .create-form button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .project-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .project-list li a {
    display: block;
    flex: 1;
    padding: 0.75rem 1rem;
    border: 1px solid #e5e5e5;
    border-radius: 6px;
    text-decoration: none;
    color: inherit;
  }

  .project-list li a:hover {
    border-color: #bbb;
    background: #fafafa;
  }

  .project-row {
    display: flex;
    gap: 0.5rem;
    align-items: stretch;
  }

  .rename {
    border: 1px solid #ddd;
    background: #fff;
    border-radius: 4px;
    padding: 0 0.6rem;
    cursor: pointer;
    font-size: 0.8rem;
    color: #555;
    font-family: inherit;
  }

  .rename:hover {
    border-color: #999;
    color: #222;
  }

  .rename:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .rename-form {
    flex: 1;
    display: flex;
  }

  .rename-input {
    flex: 1;
    padding: 0.5rem 0.75rem;
    border: 1px solid #888;
    border-radius: 4px;
    font: inherit;
    font-size: 0.9rem;
    outline: none;
    background: #fff;
    color: #222;
  }

  .name {
    font-weight: 500;
    font-size: 0.95rem;
  }

  .desc {
    display: block;
    color: #888;
    font-size: 0.82rem;
    margin-top: 0.15rem;
  }

  .badge {
    font-size: 0.68rem;
    padding: 0.1rem 0.4rem;
    border-radius: 3px;
    margin-left: 0.4rem;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .badge.archived {
    background: #fef3c7;
    color: #92400e;
  }

  .empty-panel {
    border: 1px solid #e5e5e5;
    border-radius: 6px;
    padding: 2rem;
  }

  .error {
    color: #b91c1c;
    font-size: 0.85rem;
    padding: 0.5rem 0.7rem;
    background: #fef2f2;
    border: 1px solid #fecaca;
    border-radius: 4px;
  }

  .status {
    color: #888;
    text-align: center;
    padding: 1rem;
    margin: 0;
  }
</style>
