<script lang="ts">
  import { page } from "$app/state";
  import { resolve } from "$app/paths";
  import { onMount } from "svelte";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";
  import {
    getProject$,
    listTasks$,
    createTask,
    updateTask,
    moveTask,
    deleteTask,
    unarchiveProject,
    trackExportProject,
    trackScheduleProjectArchive,
  } from "$lib/forge";
  import type {
    ForgeError,
    Task,
    TaskStatus,
    TaskPriority,
    ExportOutput,
  } from "$lib/forge";
  import { getUserId } from "$lib/auth";

  const userId = getUserId();
  const projectId = page.params.id!;
  const project = getProject$({ id: projectId, user_id: userId });
  const tasks = listTasks$({ user_id: userId, project_id: projectId });
  let localTasks: Task[] = $state([]);
  let deletedTaskIds = new SvelteSet<string>();

  let newTitle = $state("");
  let newPriority: TaskPriority = $state("medium");
  let creating = $state(false);
  let error: string | null = $state(null);
  let visibleTasks: Task[] = $derived.by(() => {
    const taskMap = new SvelteMap<string, Task>();

    for (const task of tasks.data ?? []) {
      if (!deletedTaskIds.has(task.id)) {
        taskMap.set(task.id, task);
      }
    }

    for (const task of localTasks) {
      if (!deletedTaskIds.has(task.id)) {
        taskMap.set(task.id, task);
      }
    }

    return Array.from(taskMap.values()).sort((a, b) => a.position - b.position);
  });

  let exportJob: ReturnType<typeof trackExportProject> | null = $state(null);
  let exportState: {
    loading: boolean;
    status: string;
    progress: number | null;
    message: string | null;
    output: ExportOutput | null;
    error: string | null;
  } | null = $state(null);
  let archiveWorkflow: ReturnType<typeof trackScheduleProjectArchive> | null =
    $state(null);
  let archiveWorkflowStatus = $state<string | null>(null);
  let nowMs = $state(Date.now());

  onMount(() => {
    const interval = setInterval(() => {
      nowMs = Date.now();
    }, 1000);
    return () => clearInterval(interval);
  });

  const columns: { status: TaskStatus; label: string }[] = [
    { status: "backlog", label: "Backlog" },
    { status: "todo", label: "Todo" },
    { status: "in_progress", label: "In Progress" },
    { status: "done", label: "Done" },
  ];

  const priorityColors: Record<TaskPriority, string> = {
    low: "#4ade80",
    medium: "#fbbf24",
    high: "#f87171",
  };

  function tasksByStatus(allTasks: Task[], status: TaskStatus): Task[] {
    return allTasks.filter((t) => t.status === status);
  }

  function prevStatus(status: TaskStatus): TaskStatus | null {
    const i = columns.findIndex((c) => c.status === status);
    return i > 0 ? columns[i - 1].status : null;
  }

  function nextStatus(status: TaskStatus): TaskStatus | null {
    const i = columns.findIndex((c) => c.status === status);
    return i < columns.length - 1 ? columns[i + 1].status : null;
  }

  async function handleCreateTask() {
    if (!newTitle.trim()) return;
    creating = true;
    error = null;
    try {
      const created = await createTask({
        user_id: userId,
        project_id: projectId,
        title: newTitle.trim(),
        description: "",
        priority: newPriority,
      });
      localTasks = [
        created,
        ...localTasks.filter((task) => task.id !== created.id),
      ];
      deletedTaskIds.delete(created.id);
      newTitle = "";
      newPriority = "medium";
    } catch (e) {
      error = (e as ForgeError).message;
    } finally {
      creating = false;
    }
  }

  async function handleMove(taskId: string, status: TaskStatus) {
    try {
      const moved = await moveTask({ user_id: userId, id: taskId, status });
      localTasks = [
        moved,
        ...localTasks.filter((task) => task.id !== moved.id),
      ];
    } catch (e) {
      error = (e as ForgeError).message;
    }
  }

  async function handleDelete(taskId: string) {
    try {
      await deleteTask({ user_id: userId, id: taskId });
      localTasks = localTasks.filter((task) => task.id !== taskId);
      deletedTaskIds.add(taskId);
    } catch (e) {
      error = (e as ForgeError).message;
    }
  }

  async function handleEdit(task: Task) {
    const nextTitle = prompt("Update task title", task.title);
    if (!nextTitle?.trim()) return;

    try {
      const updated = await updateTask({
        user_id: userId,
        id: task.id,
        title: nextTitle.trim(),
      });
      localTasks = [updated, ...localTasks.filter((t) => t.id !== updated.id)];
    } catch (e) {
      error = (e as ForgeError).message;
    }
  }

  function handleExport(format: "csv" | "json") {
    exportJob = trackExportProject({
      user_id: userId,
      project_id: projectId,
      format,
    });
    exportJob.subscribe((s) => {
      exportState = {
        loading: s.loading,
        status: s.status,
        progress: s.progress,
        message: s.message,
        output: s.output,
        error: s.error,
      };
    });
  }

  function handleScheduleDeletion() {
    const accepted = confirm(
      "Tasks will be exported now and permanently deleted in 7 days. You can unarchive before the deadline.",
    );
    if (!accepted) return;

    archiveWorkflow = trackScheduleProjectArchive({
      user_id: userId,
      project_id: projectId,
    });
    archiveWorkflow.subscribe((state) => {
      archiveWorkflowStatus = state.status;
      if (state.error) {
        error = state.error;
      }
    });
  }

  async function handleUnarchive() {
    try {
      await unarchiveProject({ user_id: userId, id: projectId });
      archiveWorkflowStatus = null;
    } catch (e) {
      error = (e as ForgeError).message;
    }
  }

  function formatTimeLeft(deleteAtRaw?: string): string {
    if (!deleteAtRaw) return "Not scheduled";

    const deleteAtMs = Date.parse(deleteAtRaw);
    if (Number.isNaN(deleteAtMs)) return "Unknown";

    const diffMs = deleteAtMs - nowMs;
    if (diffMs <= 0) return "Less than a minute";

    const totalMinutes = Math.floor(diffMs / 60_000);
    const days = Math.floor(totalMinutes / (60 * 24));
    const hours = Math.floor((totalMinutes % (60 * 24)) / 60);
    const minutes = totalMinutes % 60;

    if (days > 0) return `${days}d ${hours}h ${minutes}m`;
    if (hours > 0) return `${hours}h ${minutes}m`;
    return `${minutes}m`;
  }
</script>

<main>
  <div class="shell">
    <header>
      <div class="header-left">
        <a href={resolve("/app")}>&larr; Projects</a>
        {#if project.data}
          <h1>{project.data.project.name}</h1>
          {#if project.data.project.description}
            <p class="desc">{project.data.project.description}</p>
          {/if}
        {/if}
      </div>
      <div class="actions">
        <button class="export-btn" onclick={() => handleExport("json")}>
          Export JSON
        </button>
        <button class="export-btn" onclick={() => handleExport("csv")}>
          Export CSV
        </button>
        <button
          class="schedule-btn"
          onclick={handleScheduleDeletion}
          disabled={!!project.data?.project.archive_delete_at}
        >
          {project.data?.project.archive_delete_at
            ? "Deletion Scheduled"
            : "Schedule Export + Delete"}
        </button>
        {#if project.data?.project.archive_delete_at}
          <span class="badge archived">Scheduled</span>
        {:else if project.data?.project.archived}
          <span class="badge archived">Completed</span>
        {/if}
      </div>
    </header>

    {#if error}
      <p class="error">{error}</p>
    {/if}

    {#if archiveWorkflowStatus}
      <div class="job-status">
        Deletion workflow: {archiveWorkflowStatus}
      </div>
    {/if}

    {#if project.data?.project.archive_delete_at}
      <div class="archive-notice">
        <p>Tasks are exported now and scheduled for deletion in 7 days.</p>
        <p>
          Time left:
          <strong
            >{formatTimeLeft(project.data.project.archive_delete_at)}</strong
          >
        </p>
        <button class="unarchive-btn" onclick={handleUnarchive}>
          Unarchive (Cancel Delete)
        </button>
      </div>
    {/if}

    {#if exportState}
      <div class="job-status">
        Export: {exportState.status}
        {#if exportState.progress != null}
          <span class="progress-bar">
            <span class="progress-fill" style="width: {exportState.progress}%"
            ></span>
          </span>
          ({exportState.progress}%)
        {/if}
        {#if exportState.message}
          - {exportState.message}
        {/if}
        {#if exportState.error}
          <span class="error-text"> {exportState.error}</span>
        {/if}
        {#if exportState.output}
          <details>
            <summary>{exportState.output.task_count} tasks exported</summary>
            <pre>{exportState.output.data}</pre>
          </details>
        {/if}
      </div>
    {/if}

    <section class="create-section">
      <form
        class="create-form"
        onsubmit={(event) => {
          event.preventDefault();
          void handleCreateTask();
        }}
      >
        <input
          bind:value={newTitle}
          placeholder="New task title"
          disabled={creating}
        />
        <select bind:value={newPriority} disabled={creating}>
          <option value="low">Low</option>
          <option value="medium">Medium</option>
          <option value="high">High</option>
        </select>
        <button type="submit" disabled={creating || !newTitle.trim()}>
          {creating ? "..." : "Add task"}
        </button>
      </form>
    </section>

    {#if project.loading || tasks.loading}
      <p class="status">Loading...</p>
    {:else if project.error}
      <p class="error">{project.error.message}</p>
    {:else if tasks.error}
      <p class="error">{tasks.error.message}</p>
    {:else if project.data}
      <div class="board">
        {#each columns as col (col.status)}
          <div class="column">
            <h2>
              {col.label}
              <span class="count">
                {tasksByStatus(visibleTasks, col.status).length}
              </span>
            </h2>
            <div class="cards">
              {#each tasksByStatus(visibleTasks, col.status) as task (task.id)}
                <div class="card">
                  <div class="card-header">
                    <span class="title">{task.title}</span>
                    <span
                      class="priority"
                      style="background: {priorityColors[task.priority]}"
                    >
                      {task.priority}
                    </span>
                  </div>
                  {#if task.due_date}
                    <div class="due-date">Due: {task.due_date}</div>
                  {/if}
                  <div class="card-actions">
                    {#if prevStatus(task.status)}
                      <button
                        onclick={() =>
                          handleMove(task.id, prevStatus(task.status)!)}
                      >
                        &larr;
                      </button>
                    {/if}
                    {#if nextStatus(task.status)}
                      <button
                        onclick={() =>
                          handleMove(task.id, nextStatus(task.status)!)}
                      >
                        &rarr;
                      </button>
                    {/if}
                    <button class="edit" onclick={() => handleEdit(task)}>
                      Edit
                    </button>
                    <button
                      class="delete"
                      onclick={() => handleDelete(task.id)}
                    >
                      &times;
                    </button>
                  </div>
                </div>
              {/each}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</main>

<style>
  @import url("https://fonts.googleapis.com/css2?family=Fraunces:opsz,wght@9..144,700;9..144,900&family=IBM+Plex+Sans:wght@400;500;600&display=swap");

  :global(body) {
    margin: 0;
    background:
      radial-gradient(
        circle at 10% 15%,
        rgba(251, 220, 159, 0.1) 0%,
        transparent 50%
      ),
      radial-gradient(
        circle at 90% 85%,
        rgba(120, 214, 192, 0.06) 0%,
        transparent 40%
      ),
      linear-gradient(155deg, #111a1f 0%, #0a1015 50%, #141e25 100%);
    color: #eaf5f0;
    font-family: "IBM Plex Sans", sans-serif;
    min-height: 100vh;
  }

  main {
    max-width: 1280px;
    margin: 0 auto;
    padding: 0 1rem;
  }

  .shell {
    padding: 1.5rem 0 3rem;
    display: grid;
    gap: 0.85rem;
  }

  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 1rem;
    flex-wrap: wrap;
    animation: rise 0.4s ease-out;
  }

  header a {
    color: #9cc4b6;
    text-decoration: none;
    font-size: 0.82rem;
    transition: color 0.15s;
  }

  header a:hover {
    color: #fed68b;
  }

  header h1 {
    margin: 0.2rem 0 0;
    font-family: "Fraunces", serif;
    font-weight: 900;
    font-size: 1.6rem;
  }

  .desc {
    color: #8ab5a7;
    margin: 0.15rem 0 0;
    font-size: 0.88rem;
  }

  .actions {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    flex-wrap: wrap;
  }

  .export-btn {
    padding: 0.38rem 0.7rem;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.06);
    cursor: pointer;
    font-size: 0.8rem;
    color: #c2ddd3;
    font-family: inherit;
    transition: background 0.15s;
  }

  .export-btn:hover {
    background: rgba(255, 255, 255, 0.1);
  }

  .schedule-btn {
    padding: 0.38rem 0.7rem;
    border: 1px solid rgba(254, 214, 139, 0.3);
    border-radius: 8px;
    background: rgba(254, 214, 139, 0.08);
    color: #fed68b;
    cursor: pointer;
    font-size: 0.8rem;
    font-family: inherit;
    transition: background 0.15s;
  }

  .schedule-btn:hover {
    background: rgba(254, 214, 139, 0.15);
  }

  .schedule-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .badge {
    font-size: 0.68rem;
    padding: 0.18rem 0.55rem;
    border-radius: 9999px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .badge.archived {
    background: rgba(254, 214, 139, 0.15);
    color: #fed68b;
  }

  .archive-notice {
    padding: 0.85rem 1rem;
    border: 1px solid rgba(254, 214, 139, 0.25);
    background: rgba(254, 214, 139, 0.06);
    border-radius: 12px;
  }

  .archive-notice p {
    margin: 0;
    color: #e8d5a8;
    font-size: 0.88rem;
  }

  .archive-notice p + p {
    margin-top: 0.3rem;
  }

  .unarchive-btn {
    margin-top: 0.6rem;
    padding: 0.4rem 0.8rem;
    border: 1px solid rgba(254, 214, 139, 0.3);
    border-radius: 8px;
    background: rgba(254, 214, 139, 0.08);
    color: #fed68b;
    cursor: pointer;
    font-size: 0.82rem;
    font-family: inherit;
    transition: background 0.15s;
  }

  .unarchive-btn:hover {
    background: rgba(254, 214, 139, 0.15);
  }

  .create-section {
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 14px;
    background: rgba(11, 18, 24, 0.6);
    backdrop-filter: blur(6px);
    padding: 0.85rem;
    animation: rise 0.55s ease-out;
  }

  .create-form {
    display: flex;
    gap: 0.45rem;
  }

  .create-form input {
    flex: 1;
    padding: 0.55rem 0.7rem;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.06);
    color: #eef8f4;
    font: inherit;
    font-size: 0.92rem;
    outline: none;
    transition: border-color 0.2s;
  }

  .create-form input:focus {
    border-color: rgba(254, 214, 139, 0.45);
  }

  .create-form select {
    padding: 0.55rem 0.5rem;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.06);
    color: #eef8f4;
    font: inherit;
    font-size: 0.88rem;
    outline: none;
  }

  .create-form button {
    padding: 0.55rem 1rem;
    background: linear-gradient(96deg, #fed68b 0%, #78d6c0 100%);
    color: #0f1d23;
    border: 0;
    border-radius: 8px;
    cursor: pointer;
    font-weight: 700;
    font-family: inherit;
    font-size: 0.88rem;
    transition: transform 0.15s ease;
    white-space: nowrap;
  }

  .create-form button:hover {
    transform: translateY(-1px);
  }

  .create-form button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
    transform: none;
  }

  .board {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0.7rem;
    animation: rise 0.65s ease-out;
  }

  .column {
    background: rgba(11, 18, 24, 0.5);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 14px;
    padding: 0.7rem;
    min-height: 220px;
  }

  .column h2 {
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: #8ab5a7;
    margin: 0 0 0.65rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-family: "IBM Plex Sans", sans-serif;
    font-weight: 600;
  }

  .count {
    background: rgba(255, 255, 255, 0.08);
    padding: 0.1rem 0.45rem;
    border-radius: 9999px;
    font-size: 0.7rem;
    color: #a8d4c4;
  }

  .cards {
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
  }

  .card {
    background: rgba(20, 30, 37, 0.8);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 10px;
    padding: 0.65rem;
    transition: border-color 0.2s;
  }

  .card:hover {
    border-color: rgba(255, 255, 255, 0.18);
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 0.4rem;
  }

  .title {
    font-weight: 500;
    font-size: 0.85rem;
    word-break: break-word;
    line-height: 1.3;
  }

  .priority {
    font-size: 0.6rem;
    padding: 0.1rem 0.35rem;
    border-radius: 9999px;
    color: #0f1d23;
    text-transform: uppercase;
    font-weight: 700;
    white-space: nowrap;
    letter-spacing: 0.04em;
  }

  .due-date {
    font-size: 0.72rem;
    color: #8ab5a7;
    margin-top: 0.3rem;
  }

  .card-actions {
    display: flex;
    gap: 0.2rem;
    margin-top: 0.45rem;
    opacity: 0;
    transition: opacity 0.15s;
  }

  .card:hover .card-actions {
    opacity: 1;
  }

  .card-actions button {
    padding: 0.15rem 0.45rem;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 5px;
    background: rgba(255, 255, 255, 0.04);
    cursor: pointer;
    font-size: 0.8rem;
    color: #b0cfc4;
    font-family: inherit;
    transition: background 0.12s;
  }

  .card-actions button:hover {
    background: rgba(255, 255, 255, 0.1);
  }

  .card-actions .delete {
    margin-left: auto;
    color: #f87171;
    border-color: rgba(248, 113, 113, 0.2);
  }

  .card-actions .delete:hover {
    background: rgba(248, 113, 113, 0.1);
  }

  .card-actions .edit {
    color: #78d6c0;
    border-color: rgba(120, 214, 192, 0.2);
  }

  .card-actions .edit:hover {
    background: rgba(120, 214, 192, 0.1);
  }

  .job-status {
    padding: 0.75rem 0.9rem;
    background: rgba(120, 214, 192, 0.06);
    border: 1px solid rgba(120, 214, 192, 0.15);
    border-radius: 10px;
    font-size: 0.85rem;
    color: #a8d4c4;
  }

  .job-status pre {
    margin: 0.4rem 0 0;
    font-size: 0.72rem;
    max-height: 200px;
    overflow: auto;
    background: rgba(0, 0, 0, 0.3);
    padding: 0.5rem;
    border-radius: 6px;
    color: #c2ddd3;
  }

  .job-status details summary {
    cursor: pointer;
    margin-top: 0.3rem;
    color: #78d6c0;
    font-size: 0.82rem;
  }

  .progress-bar {
    display: inline-block;
    width: 60px;
    height: 4px;
    background: rgba(255, 255, 255, 0.08);
    border-radius: 2px;
    vertical-align: middle;
    margin: 0 0.3rem;
    overflow: hidden;
  }

  .progress-fill {
    display: block;
    height: 100%;
    background: linear-gradient(90deg, #78d6c0, #fed68b);
    border-radius: 2px;
    transition: width 0.3s ease;
  }

  .error {
    color: #ffd7c7;
    font-size: 0.88rem;
    padding: 0.55rem 0.7rem;
    background: rgba(188, 63, 32, 0.25);
    border: 1px solid rgba(255, 184, 161, 0.25);
    border-radius: 10px;
  }

  .error-text {
    color: #f87171;
  }

  .status {
    color: #8ab5a7;
    text-align: center;
    padding: 2rem;
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

  @media (max-width: 860px) {
    .board {
      grid-template-columns: repeat(2, 1fr);
    }
  }

  @media (max-width: 540px) {
    .board {
      grid-template-columns: 1fr;
    }

    header {
      flex-direction: column;
    }

    .create-form {
      flex-wrap: wrap;
    }

    .create-form input {
      min-width: 0;
    }
  }
</style>
