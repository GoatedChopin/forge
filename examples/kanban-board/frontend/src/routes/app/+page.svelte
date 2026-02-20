<script lang="ts">
  import { goto } from "$app/navigation";
  import { resolve } from "$app/paths";
  import { SvelteMap } from "svelte/reactivity";
  import {
    getForgeClient,
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

  async function handleRename(project: { id: string; name: string }) {
    const next = prompt("Rename project", project.name);
    if (!next) return;

    try {
      const updated = await updateProject({
        user_id: userId,
        id: project.id,
        name: next.trim(),
      });
      localProjects = [
        updated,
        ...localProjects.filter((p) => p.id !== updated.id),
      ];
    } catch (e) {
      error = (e as ForgeError).message;
    }
  }

  async function handleLogout() {
    localStorage.removeItem("kanban_token");
    localStorage.removeItem("kanban_user");
    await getForgeClient().reconnect();
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
          {#each visibleProjects as project, i (project.id)}
            <li style="animation-delay: {i * 50}ms">
              <div class="project-row">
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
  </div>
</main>

<style>
  @import url("https://fonts.googleapis.com/css2?family=Fraunces:opsz,wght@9..144,700;9..144,900&family=IBM+Plex+Sans:wght@400;500;600&display=swap");

  :global(body) {
    margin: 0;
    background:
      radial-gradient(
        circle at 20% 25%,
        rgba(251, 220, 159, 0.12) 0%,
        transparent 50%
      ),
      radial-gradient(
        circle at 80% 75%,
        rgba(120, 214, 192, 0.08) 0%,
        transparent 45%
      ),
      linear-gradient(155deg, #111a1f 0%, #0a1015 50%, #141e25 100%);
    color: #eaf5f0;
    font-family: "IBM Plex Sans", sans-serif;
    min-height: 100vh;
  }

  main {
    max-width: 640px;
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
    animation: rise 0.5s ease-out;
  }

  h1 {
    margin: 0;
    font-family: "Fraunces", serif;
    font-weight: 900;
    font-size: 1.8rem;
  }

  .subtitle {
    margin: 0.15rem 0 0;
    font-size: 0.82rem;
    color: #9cc4b6;
    letter-spacing: 0.04em;
  }

  .logout {
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    padding: 0.4rem 0.85rem;
    border-radius: 10px;
    cursor: pointer;
    font-size: 0.82rem;
    color: #c2ddd3;
    font-family: inherit;
    transition: background 0.15s;
  }

  .logout:hover {
    background: rgba(255, 255, 255, 0.12);
  }

  .create-section {
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 16px;
    background: rgba(11, 18, 24, 0.7);
    backdrop-filter: blur(8px);
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.25);
    padding: 1rem;
    animation: rise 0.6s ease-out;
  }

  .create-form {
    display: flex;
    gap: 0.5rem;
  }

  .create-form input {
    flex: 1;
    padding: 0.6rem 0.75rem;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.06);
    color: #eef8f4;
    font: inherit;
    font-size: 0.95rem;
    outline: none;
    transition: border-color 0.2s;
  }

  .create-form input:focus {
    border-color: rgba(254, 214, 139, 0.45);
  }

  .create-form button {
    padding: 0.6rem 1.1rem;
    background: linear-gradient(96deg, #fed68b 0%, #78d6c0 100%);
    color: #0f1d23;
    border: 0;
    border-radius: 10px;
    cursor: pointer;
    font-weight: 700;
    font-family: inherit;
    transition: transform 0.15s ease;
  }

  .create-form button:hover {
    transform: translateY(-1px);
  }

  .create-form button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
    transform: none;
  }

  .project-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
  }

  .project-list li {
    animation: slideIn 0.4s ease-out both;
  }

  .project-list li a {
    display: block;
    flex: 1;
    padding: 1rem 1.1rem;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 14px;
    text-decoration: none;
    color: inherit;
    background: rgba(11, 18, 24, 0.6);
    backdrop-filter: blur(6px);
    transition:
      border-color 0.2s,
      background 0.2s;
  }

  .project-list li a:hover {
    border-color: rgba(254, 214, 139, 0.35);
    background: rgba(20, 30, 37, 0.8);
  }

  .project-row {
    display: flex;
    gap: 0.5rem;
    align-items: stretch;
  }

  .rename {
    border: 1px solid rgba(255, 255, 255, 0.12);
    background: rgba(255, 255, 255, 0.06);
    border-radius: 10px;
    padding: 0 0.75rem;
    cursor: pointer;
    font-size: 0.82rem;
    color: #b0cfc4;
    font-family: inherit;
    transition: background 0.15s;
  }

  .rename:hover {
    background: rgba(255, 255, 255, 0.1);
  }

  .rename:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .name {
    font-weight: 600;
    font-size: 1.05rem;
  }

  .desc {
    display: block;
    color: #8ab5a7;
    font-size: 0.85rem;
    margin-top: 0.2rem;
  }

  .badge {
    font-size: 0.7rem;
    padding: 0.15rem 0.5rem;
    border-radius: 9999px;
    margin-left: 0.5rem;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .badge.archived {
    background: rgba(254, 214, 139, 0.15);
    color: #fed68b;
  }

  .empty-panel {
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 16px;
    background: rgba(11, 18, 24, 0.5);
    padding: 2rem;
  }

  .error {
    color: #ffd7c7;
    font-size: 0.88rem;
    padding: 0.55rem 0.7rem;
    background: rgba(188, 63, 32, 0.25);
    border: 1px solid rgba(255, 184, 161, 0.25);
    border-radius: 10px;
  }

  .status {
    color: #8ab5a7;
    text-align: center;
    padding: 1rem;
    margin: 0;
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(10px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  @keyframes slideIn {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
