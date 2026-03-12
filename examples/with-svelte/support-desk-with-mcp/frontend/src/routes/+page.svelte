<script lang="ts">
  import {
    listSupportTickets$,
    createSupportTicket,
    setTicketStatus,
    setTicketPriority,
    addTicketNote,
  } from "$lib/forge";
  import type {
    ForgeError,
    SupportTicket,
    TicketPriority,
    TicketStatus,
  } from "$lib/forge";

  const tickets = listSupportTickets$();

  let customerName: string = $state("");
  let title: string = $state("");
  let details: string = $state("");
  let priority: TicketPriority = $state("normal");

  let submitError: ForgeError | null = $state(null);
  let actionError: ForgeError | null = $state(null);
  let creating: boolean = $state(false);
  let busyKey: string | null = $state(null);
  let noteDrafts: Record<string, string> = $state({});

  function laneLabel(status: TicketStatus): string {
    if (status === "new") return "Fresh";
    if (status === "working") return "In Flight";
    return "Closed";
  }

  async function createTicket() {
    if (!customerName.trim() || !title.trim() || !details.trim()) {
      return;
    }

    submitError = null;
    creating = true;
    try {
      await createSupportTicket({
        customer_name: customerName,
        title,
        details,
        priority,
      });
      customerName = "";
      title = "";
      details = "";
      priority = "normal";
    } catch (error) {
      submitError = error as ForgeError;
    } finally {
      creating = false;
    }
  }

  async function updateStatus(id: string, status: TicketStatus) {
    actionError = null;
    busyKey = `${id}:status`;
    try {
      await setTicketStatus({ id, status });
    } catch (error) {
      actionError = error as ForgeError;
    } finally {
      busyKey = null;
    }
  }

  async function updatePriority(id: string, nextPriority: TicketPriority) {
    actionError = null;
    busyKey = `${id}:priority`;
    try {
      await setTicketPriority({ id, priority: nextPriority });
    } catch (error) {
      actionError = error as ForgeError;
    } finally {
      busyKey = null;
    }
  }

  async function saveNote(ticket: SupportTicket) {
    const note = (noteDrafts[ticket.id] ?? "").trim();
    if (!note) {
      return;
    }

    actionError = null;
    busyKey = `${ticket.id}:note`;
    try {
      await addTicketNote({ id: ticket.id, note });
      noteDrafts = { ...noteDrafts, [ticket.id]: "" };
    } catch (error) {
      actionError = error as ForgeError;
    } finally {
      busyKey = null;
    }
  }
</script>

<main class="desk-shell">
  <header class="hero">
    <h1>Support Desk</h1>
    <p class="lede">
      Shared inbox for humans and LLM agents. MCP endpoint at
      <code>/_api/mcp</code>.
    </p>
  </header>

  <section class="create-panel">
    <h2>Open Ticket</h2>
    <div class="grid">
      <label>
        Customer
        <input
          data-testid="customer-name-input"
          bind:value={customerName}
          placeholder="Keisha Moore"
        />
      </label>
      <label>
        Subject
        <input
          data-testid="subject-input"
          bind:value={title}
          placeholder="Export job stalling"
        />
      </label>
      <label class="span-2">
        Details
        <textarea
          data-testid="details-input"
          bind:value={details}
          rows="3"
          placeholder="Customer can reproduce every time on Safari 17."
        ></textarea>
      </label>
      <label>
        Priority
        <select data-testid="priority-select" bind:value={priority}>
          <option value="low">Low</option>
          <option value="normal">Normal</option>
          <option value="high">High</option>
        </select>
      </label>
      <div class="actions">
        <button
          class="btn-primary"
          data-testid="open-ticket-button"
          onclick={createTicket}
          disabled={creating ||
            !customerName.trim() ||
            !title.trim() ||
            !details.trim()}
        >
          {creating ? "Opening..." : "Open Ticket"}
        </button>
      </div>
    </div>

    {#if submitError}
      <p class="error">{submitError.message}</p>
    {/if}
  </section>

  <section class="board">
    <div class="board-head">
      <h2>Live Inbox</h2>
      {#if tickets.data}
        <span>{tickets.data.length} total</span>
      {/if}
    </div>

    {#if actionError}
      <p class="error">{actionError.message}</p>
    {/if}

    {#if tickets.loading}
      <p class="status">Loading ticket lanes...</p>
    {:else if tickets.error}
      <p class="error">{tickets.error.message}</p>
    {:else if tickets.data && tickets.data.length === 0}
      <p class="status">No tickets yet. Open one above.</p>
    {:else if tickets.data}
      <ul class="ticket-list">
        {#each tickets.data as ticket (ticket.id)}
          <li class="ticket" data-testid="ticket-card">
            <header>
              <div>
                <p class="customer">{ticket.customer_name}</p>
                <h3>{ticket.title}</h3>
              </div>
              <div class="chips">
                <span class="chip status-chip status-{ticket.status}">
                  {laneLabel(ticket.status)}
                </span>
                <span class="chip priority-chip priority-{ticket.priority}">
                  {ticket.priority}
                </span>
              </div>
            </header>

            <p class="details">{ticket.details}</p>
            {#if ticket.last_note}
              <p class="note">Latest note: {ticket.last_note}</p>
            {/if}

            <div class="controls">
              <div class="button-row">
                <button
                  class="ghost"
                  onclick={() => updateStatus(ticket.id, "working")}
                  disabled={busyKey === `${ticket.id}:status`}
                >
                  Start Work
                </button>
                <button
                  class="ghost"
                  onclick={() => updateStatus(ticket.id, "resolved")}
                  disabled={busyKey === `${ticket.id}:status`}
                >
                  Resolve
                </button>
                <button
                  class="ghost"
                  onclick={() => updateStatus(ticket.id, "new")}
                  disabled={busyKey === `${ticket.id}:status`}
                >
                  Reopen
                </button>
                <button
                  class="ghost"
                  onclick={() => updatePriority(ticket.id, "high")}
                  disabled={busyKey === `${ticket.id}:priority`}
                >
                  Escalate
                </button>
                <button
                  class="ghost"
                  onclick={() => updatePriority(ticket.id, "normal")}
                  disabled={busyKey === `${ticket.id}:priority`}
                >
                  Set Normal
                </button>
              </div>

              <div class="note-row">
                <input
                  value={noteDrafts[ticket.id] ?? ""}
                  oninput={(event) => {
                    const target = event.currentTarget as HTMLInputElement;
                    noteDrafts = { ...noteDrafts, [ticket.id]: target.value };
                  }}
                  placeholder="Add latest internal note"
                />
                <button
                  class="ghost"
                  onclick={() => saveNote(ticket)}
                  disabled={busyKey === `${ticket.id}:note`}
                >
                  Save Note
                </button>
              </div>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
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

  .desk-shell {
    width: min(960px, calc(100% - 2rem));
    margin: 1.5rem auto 2.5rem;
    display: grid;
    gap: 1rem;
  }

  .hero {
    padding: 0 0 0.75rem;
    border-bottom: 1px solid #e5e5e5;
  }

  h1 {
    margin: 0;
    font-weight: 600;
    font-size: 1.5rem;
    color: #111;
  }

  .lede {
    margin: 0.25rem 0 0;
    color: #666;
    font-size: 0.88rem;
  }

  .create-panel {
    border: 1px solid #e5e5e5;
    border-radius: 6px;
    padding: 1rem;
  }

  .create-panel h2,
  .board h2 {
    margin: 0 0 0.75rem;
    font-weight: 600;
    font-size: 1.1rem;
    color: #111;
  }

  .grid {
    display: grid;
    gap: 0.65rem;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  label {
    display: grid;
    gap: 0.25rem;
    font-size: 0.82rem;
    color: #555;
  }

  input,
  textarea,
  select {
    border: 1px solid #ccc;
    border-radius: 4px;
    padding: 0.5rem 0.65rem;
    background: #fff;
    color: #222;
    font: inherit;
    font-size: 0.88rem;
    outline: none;
  }

  input:focus,
  textarea:focus,
  select:focus {
    border-color: #888;
  }

  textarea {
    resize: vertical;
  }

  .span-2 {
    grid-column: span 2;
  }

  .actions {
    display: flex;
    align-items: end;
  }

  .btn-primary {
    border: none;
    border-radius: 4px;
    padding: 0.5rem 1rem;
    background: #111;
    color: #fff;
    font-weight: 500;
    cursor: pointer;
    font-family: inherit;
    font-size: 0.88rem;
  }

  .btn-primary:hover {
    background: #333;
  }

  .btn-primary:disabled {
    cursor: not-allowed;
    opacity: 0.4;
  }

  .board {
    border: 1px solid #e5e5e5;
    border-radius: 6px;
    padding: 1rem;
  }

  .board-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .board-head span {
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: #888;
  }

  .ticket-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.6rem;
  }

  .ticket {
    border: 1px solid #e5e5e5;
    border-radius: 6px;
    padding: 0.75rem;
  }

  .ticket header {
    display: flex;
    justify-content: space-between;
    align-items: start;
    gap: 0.75rem;
  }

  .customer {
    margin: 0;
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: #888;
  }

  h3 {
    margin: 0.15rem 0 0;
    font-size: 0.95rem;
    font-weight: 600;
    line-height: 1.2;
  }

  .chips {
    display: flex;
    gap: 0.3rem;
    flex-wrap: wrap;
  }

  .chip {
    border-radius: 3px;
    padding: 0.15rem 0.45rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-weight: 500;
  }

  .status-new {
    background: #fffbeb;
    color: #92400e;
  }

  .status-working {
    background: #f0f9ff;
    color: #0c4a6e;
  }

  .status-resolved {
    background: #f0fdf4;
    color: #166534;
  }

  .priority-low {
    background: #f0fdf4;
    color: #166534;
  }

  .priority-normal {
    background: #f0f9ff;
    color: #0c4a6e;
  }

  .priority-high {
    background: #fef2f2;
    color: #991b1b;
  }

  .details {
    margin: 0.4rem 0 0;
    color: #444;
    font-size: 0.88rem;
  }

  .note {
    margin: 0.35rem 0 0;
    font-size: 0.82rem;
    color: #92400e;
  }

  .controls {
    margin-top: 0.6rem;
    display: grid;
    gap: 0.45rem;
  }

  .button-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
  }

  .ghost {
    background: #f5f5f5;
    color: #333;
    border: 1px solid #ddd;
    border-radius: 4px;
    font-size: 0.78rem;
    padding: 0.3rem 0.55rem;
    font-weight: 500;
    cursor: pointer;
    font-family: inherit;
  }

  .ghost:hover {
    background: #eee;
    border-color: #bbb;
  }

  .ghost:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .note-row {
    display: grid;
    gap: 0.4rem;
    grid-template-columns: 1fr auto;
  }

  .status {
    margin: 0;
    color: #888;
    padding: 0.6rem;
    text-align: center;
    font-size: 0.88rem;
  }

  .error {
    margin: 0.5rem 0 0;
    color: #b91c1c;
    background: #fef2f2;
    border: 1px solid #fecaca;
    border-radius: 4px;
    padding: 0.5rem 0.6rem;
    font-size: 0.85rem;
  }

  code {
    font-family: ui-monospace, monospace;
    background: #f5f5f5;
    border: 1px solid #e5e5e5;
    border-radius: 3px;
    padding: 0.1rem 0.3rem;
    font-size: 0.85em;
  }

  @media (max-width: 760px) {
    .desk-shell {
      width: min(960px, calc(100% - 1rem));
    }

    .grid {
      grid-template-columns: 1fr;
    }

    .span-2 {
      grid-column: span 1;
    }

    .note-row {
      grid-template-columns: 1fr;
    }

    .ticket header {
      flex-direction: column;
    }
  }
</style>
