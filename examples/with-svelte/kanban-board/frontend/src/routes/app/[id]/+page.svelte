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
  let editingTaskId: string | null = $state(null);
  let editingTaskTitle = $state("");
  let showDeleteConfirm = $state(false);
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

  const priorityLabels: Record<TaskPriority, string> = {
    low: "Low",
    medium: "Med",
    high: "High",
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

  function startEdit(task: Task) {
    editingTaskId = task.id;
    editingTaskTitle = task.title;
  }

  async function submitEdit(taskId: string) {
    if (!editingTaskTitle.trim()) {
      editingTaskId = null;
      return;
    }
    try {
      const updated = await updateTask({
        user_id: userId,
        id: taskId,
        title: editingTaskTitle.trim(),
      });
      localTasks = [updated, ...localTasks.filter((t) => t.id !== updated.id)];
    } catch (e) {
      error = (e as ForgeError).message;
    } finally {
      editingTaskId = null;
    }
  }

  function handleEditKeydown(event: KeyboardEvent, taskId: string) {
    if (event.key === "Enter") {
      void submitEdit(taskId);
    } else if (event.key === "Escape") {
      editingTaskId = null;
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
    showDeleteConfirm = true;
  }

  function confirmScheduleDeletion() {
    showDeleteConfirm = false;
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
        <button class="btn-secondary" onclick={() => handleExport("json")}>
          Export JSON
        </button>
        <button class="btn-secondary" onclick={() => handleExport("csv")}>
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

    {#if showDeleteConfirm}
      <div class="confirm-banner">
        <p>
          Tasks will be exported and permanently deleted in 7 days. You can
          unarchive before the deadline.
        </p>
        <div class="confirm-actions">
          <button class="btn-danger" onclick={confirmScheduleDeletion}
            >Confirm Delete</button
          >
          <button
            class="btn-secondary"
            onclick={() => (showDeleteConfirm = false)}>Cancel</button
          >
        </div>
      </div>
    {/if}

    {#if archiveWorkflowStatus}
      <div class="job-status">
        Deletion workflow: {archiveWorkflowStatus}
      </div>
    {/if}

    {#if project.data?.project.archive_delete_at}
      <div class="archive-notice">
        <p>Tasks are exported and scheduled for deletion in 7 days.</p>
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
                    {#if editingTaskId === task.id}
                      <input
                        class="edit-input"
                        bind:value={editingTaskTitle}
                        onkeydown={(e) => handleEditKeydown(e, task.id)}
                        onblur={() => submitEdit(task.id)}
                      />
                    {:else}
                      <span class="title">{task.title}</span>
                    {/if}
                    <span class="priority priority-{task.priority}">
                      {priorityLabels[task.priority]}
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
                    <button class="edit" onclick={() => startEdit(task)}>
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
  :global(body) {
    margin: 0;
    background: #fff;
    color: #222;
    font-family:
      -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    min-height: 100vh;
  }

  main {
    max-width: 1200px;
    margin: 0 auto;
    padding: 0 1rem;
  }

  .shell {
    padding: 1.5rem 0 3rem;
    display: grid;
    gap: 0.75rem;
  }

  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 1rem;
    flex-wrap: wrap;
  }

  header a {
    color: #888;
    text-decoration: none;
    font-size: 0.82rem;
  }

  header a:hover {
    color: #111;
  }

  header h1 {
    margin: 0.15rem 0 0;
    font-weight: 600;
    font-size: 1.4rem;
    color: #111;
  }

  .desc {
    color: #888;
    margin: 0.1rem 0 0;
    font-size: 0.85rem;
  }

  .actions {
    display: flex;
    gap: 0.35rem;
    align-items: center;
    flex-wrap: wrap;
  }

  .btn-secondary {
    padding: 0.35rem 0.6rem;
    border: 1px solid #ddd;
    border-radius: 4px;
    background: #fff;
    cursor: pointer;
    font-size: 0.78rem;
    color: #555;
    font-family: inherit;
  }

  .btn-secondary:hover {
    border-color: #999;
    color: #222;
  }

  .schedule-btn {
    padding: 0.35rem 0.6rem;
    border: 1px solid #e5c07b;
    border-radius: 4px;
    background: #fffbeb;
    color: #92400e;
    cursor: pointer;
    font-size: 0.78rem;
    font-family: inherit;
  }

  .schedule-btn:hover {
    background: #fef3c7;
  }

  .schedule-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .badge {
    font-size: 0.68rem;
    padding: 0.12rem 0.4rem;
    border-radius: 3px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .badge.archived {
    background: #fef3c7;
    color: #92400e;
  }

  .confirm-banner {
    padding: 0.75rem 1rem;
    border: 1px solid #fecaca;
    background: #fef2f2;
    border-radius: 6px;
  }

  .confirm-banner p {
    margin: 0 0 0.5rem;
    font-size: 0.88rem;
    color: #991b1b;
  }

  .confirm-actions {
    display: flex;
    gap: 0.4rem;
  }

  .btn-danger {
    padding: 0.35rem 0.7rem;
    border: none;
    border-radius: 4px;
    background: #dc2626;
    color: #fff;
    cursor: pointer;
    font-size: 0.82rem;
    font-family: inherit;
    font-weight: 500;
  }

  .btn-danger:hover {
    background: #b91c1c;
  }

  .archive-notice {
    padding: 0.75rem 1rem;
    border: 1px solid #e5c07b;
    background: #fffbeb;
    border-radius: 6px;
  }

  .archive-notice p {
    margin: 0;
    color: #92400e;
    font-size: 0.85rem;
  }

  .archive-notice p + p {
    margin-top: 0.2rem;
  }

  .unarchive-btn {
    margin-top: 0.5rem;
    padding: 0.35rem 0.7rem;
    border: 1px solid #e5c07b;
    border-radius: 4px;
    background: #fff;
    color: #92400e;
    cursor: pointer;
    font-size: 0.82rem;
    font-family: inherit;
  }

  .unarchive-btn:hover {
    background: #fef3c7;
  }

  .create-section {
    border: 1px solid #e5e5e5;
    border-radius: 6px;
    padding: 0.75rem;
  }

  .create-form {
    display: flex;
    gap: 0.4rem;
  }

  .create-form input {
    flex: 1;
    padding: 0.45rem 0.7rem;
    border: 1px solid #ccc;
    border-radius: 4px;
    background: #fff;
    color: #222;
    font: inherit;
    font-size: 0.88rem;
    outline: none;
  }

  .create-form input:focus {
    border-color: #888;
  }

  .create-form select {
    padding: 0.45rem 0.4rem;
    border: 1px solid #ccc;
    border-radius: 4px;
    background: #fff;
    color: #222;
    font: inherit;
    font-size: 0.85rem;
    outline: none;
  }

  .create-form button {
    padding: 0.45rem 0.85rem;
    background: #111;
    color: #fff;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-weight: 500;
    font-family: inherit;
    font-size: 0.85rem;
    white-space: nowrap;
  }

  .create-form button:hover {
    background: #333;
  }

  .create-form button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .board {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0.6rem;
  }

  .column {
    background: #f9f9f9;
    border: 1px solid #e5e5e5;
    border-radius: 6px;
    padding: 0.6rem;
    min-height: 200px;
  }

  .column h2 {
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: #888;
    margin: 0 0 0.5rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-weight: 600;
  }

  .count {
    background: #eee;
    padding: 0.08rem 0.35rem;
    border-radius: 3px;
    font-size: 0.68rem;
    color: #666;
  }

  .cards {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .card {
    background: #fff;
    border: 1px solid #e5e5e5;
    border-radius: 4px;
    padding: 0.55rem;
  }

  .card:hover {
    border-color: #ccc;
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 0.35rem;
  }

  .title {
    font-weight: 500;
    font-size: 0.82rem;
    word-break: break-word;
    line-height: 1.3;
  }

  .edit-input {
    flex: 1;
    padding: 0.2rem 0.4rem;
    border: 1px solid #888;
    border-radius: 3px;
    font: inherit;
    font-size: 0.82rem;
    outline: none;
    background: #fff;
    color: #222;
  }

  .priority {
    font-size: 0.6rem;
    padding: 0.08rem 0.3rem;
    border-radius: 3px;
    text-transform: uppercase;
    font-weight: 600;
    white-space: nowrap;
    letter-spacing: 0.03em;
  }

  .priority-low {
    background: #f0fdf4;
    color: #166534;
  }

  .priority-medium {
    background: #fffbeb;
    color: #92400e;
  }

  .priority-high {
    background: #fef2f2;
    color: #991b1b;
  }

  .due-date {
    font-size: 0.7rem;
    color: #888;
    margin-top: 0.25rem;
  }

  .card-actions {
    display: flex;
    gap: 0.15rem;
    margin-top: 0.35rem;
    opacity: 0;
    transition: opacity 0.1s;
  }

  .card:hover .card-actions {
    opacity: 1;
  }

  .card-actions button {
    padding: 0.12rem 0.35rem;
    border: 1px solid #ddd;
    border-radius: 3px;
    background: #fff;
    cursor: pointer;
    font-size: 0.75rem;
    color: #555;
    font-family: inherit;
  }

  .card-actions button:hover {
    border-color: #999;
    color: #222;
  }

  .card-actions .delete {
    margin-left: auto;
    color: #dc2626;
    border-color: #fecaca;
  }

  .card-actions .delete:hover {
    background: #fef2f2;
  }

  .card-actions .edit {
    color: #2563eb;
    border-color: #bfdbfe;
  }

  .card-actions .edit:hover {
    background: #eff6ff;
  }

  .job-status {
    padding: 0.6rem 0.8rem;
    background: #f0f9ff;
    border: 1px solid #bae6fd;
    border-radius: 6px;
    font-size: 0.82rem;
    color: #0c4a6e;
  }

  .job-status pre {
    margin: 0.3rem 0 0;
    font-size: 0.7rem;
    max-height: 200px;
    overflow: auto;
    background: #f5f5f5;
    padding: 0.4rem;
    border-radius: 4px;
    color: #333;
  }

  .job-status details summary {
    cursor: pointer;
    margin-top: 0.25rem;
    color: #2563eb;
    font-size: 0.8rem;
  }

  .progress-bar {
    display: inline-block;
    width: 50px;
    height: 3px;
    background: #e5e5e5;
    border-radius: 2px;
    vertical-align: middle;
    margin: 0 0.25rem;
    overflow: hidden;
  }

  .progress-fill {
    display: block;
    height: 100%;
    background: #2563eb;
    border-radius: 2px;
  }

  .error {
    color: #b91c1c;
    font-size: 0.85rem;
    padding: 0.5rem 0.7rem;
    background: #fef2f2;
    border: 1px solid #fecaca;
    border-radius: 4px;
  }

  .error-text {
    color: #dc2626;
  }

  .status {
    color: #888;
    text-align: center;
    padding: 2rem;
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
