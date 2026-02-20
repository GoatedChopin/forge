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
  <section class="hero">
    <p class="kicker">Support Desk Example</p>
    <h1>Human and LLM share the same inbox actions.</h1>
    <p class="lede">
      Agents use this UI, while an MCP client can run equivalent actions at
      <code>/_api/mcp</code>.
    </p>
  </section>

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
                <span class="chip status status-{ticket.status}">
                  {laneLabel(ticket.status)}
                </span>
                <span class="chip priority priority-{ticket.priority}">
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
  @import url("https://fonts.googleapis.com/css2?family=Fraunces:opsz,wght@9..144,700;9..144,900&family=IBM+Plex+Sans:wght@400;500;600&display=swap");

  :global(body) {
    margin: 0;
    background:
      radial-gradient(circle at 15% 20%, #fbdc9f 0%, transparent 45%),
      radial-gradient(circle at 82% 12%, #9fd5c4 0%, transparent 42%),
      linear-gradient(145deg, #152126 0%, #0d1419 55%, #1b2830 100%);
    color: #eaf5f0;
    font-family: "IBM Plex Sans", sans-serif;
    min-height: 100vh;
  }

  .desk-shell {
    width: min(1040px, calc(100% - 2.25rem));
    margin: 1.8rem auto 2.6rem;
    display: grid;
    gap: 1rem;
  }

  .hero,
  .create-panel,
  .board {
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 20px;
    background: rgba(11, 18, 24, 0.74);
    backdrop-filter: blur(7px);
    box-shadow: 0 18px 45px rgba(0, 0, 0, 0.32);
  }

  .hero {
    padding: 1.4rem 1.45rem;
    animation: rise 0.55s ease-out;
  }

  .kicker {
    margin: 0;
    font-size: 0.74rem;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: #c9e9de;
  }

  h1 {
    margin: 0.4rem 0;
    font-family: "Fraunces", serif;
    font-weight: 900;
    line-height: 1.1;
    font-size: clamp(1.7rem, 4vw, 2.6rem);
    max-width: 18ch;
  }

  .lede {
    margin: 0;
    color: #bed8cf;
  }

  .create-panel {
    padding: 1.2rem 1.1rem;
    animation: rise 0.68s ease-out;
  }

  .create-panel h2,
  .board h2 {
    margin: 0 0 0.9rem;
    font-family: "Fraunces", serif;
    font-size: 1.3rem;
  }

  .grid {
    display: grid;
    gap: 0.8rem;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  label {
    display: grid;
    gap: 0.38rem;
    font-size: 0.83rem;
    color: #cde0d8;
  }

  input,
  textarea,
  select {
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 12px;
    padding: 0.6rem 0.72rem;
    background: rgba(255, 255, 255, 0.06);
    color: #eff8f5;
    font: inherit;
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

  button {
    border: 0;
    border-radius: 999px;
    padding: 0.62rem 1rem;
    background: linear-gradient(96deg, #fed68b 0%, #78d6c0 100%);
    color: #0f1d23;
    font-weight: 700;
    letter-spacing: 0.01em;
    cursor: pointer;
    transition: transform 0.18s ease;
  }

  button:hover {
    transform: translateY(-1px);
  }

  button:disabled {
    cursor: not-allowed;
    opacity: 0.56;
    transform: none;
  }

  .board {
    padding: 1.1rem;
    animation: rise 0.8s ease-out;
  }

  .board-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .board-head span {
    font-size: 0.78rem;
    letter-spacing: 0.11em;
    text-transform: uppercase;
    color: #b9d9cf;
  }

  .ticket-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.72rem;
  }

  .ticket {
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 14px;
    padding: 0.88rem;
    background: rgba(4, 9, 14, 0.45);
  }

  .ticket header {
    display: flex;
    justify-content: space-between;
    align-items: start;
    gap: 0.8rem;
  }

  .customer {
    margin: 0;
    font-size: 0.77rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: #abcbbe;
  }

  h3 {
    margin: 0.26rem 0 0;
    font-size: 1.05rem;
    line-height: 1.2;
  }

  .chips {
    display: flex;
    gap: 0.35rem;
    flex-wrap: wrap;
  }

  .chip {
    border-radius: 999px;
    padding: 0.22rem 0.56rem;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .status-new {
    background: rgba(255, 241, 204, 0.17);
    color: #ffe29e;
  }

  .status-working {
    background: rgba(162, 227, 255, 0.17);
    color: #9fd9f3;
  }

  .status-resolved {
    background: rgba(170, 242, 185, 0.17);
    color: #9be0aa;
  }

  .priority-low {
    background: rgba(196, 221, 214, 0.18);
    color: #c9ebe1;
  }

  .priority-normal {
    background: rgba(201, 215, 252, 0.2);
    color: #c7d7ff;
  }

  .priority-high {
    background: rgba(255, 174, 154, 0.21);
    color: #ffb39f;
  }

  .details {
    margin: 0.58rem 0 0;
    color: #d4ebe2;
  }

  .note {
    margin: 0.52rem 0 0;
    font-size: 0.86rem;
    color: #e8bf8e;
  }

  .controls {
    margin-top: 0.72rem;
    display: grid;
    gap: 0.58rem;
  }

  .button-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.42rem;
  }

  .ghost {
    background: rgba(255, 255, 255, 0.1);
    color: #ecf7f2;
    border-radius: 10px;
    font-size: 0.8rem;
    padding: 0.4rem 0.62rem;
    font-weight: 600;
  }

  .note-row {
    display: grid;
    gap: 0.55rem;
    grid-template-columns: 1fr auto;
  }

  .status {
    margin: 0;
    color: #c2ddd3;
    padding: 0.72rem;
    border-radius: 12px;
    background: rgba(255, 255, 255, 0.07);
  }

  .error {
    margin: 0.72rem 0 0;
    color: #ffd7c7;
    background: rgba(188, 63, 32, 0.28);
    border: 1px solid rgba(255, 184, 161, 0.35);
    border-radius: 11px;
    padding: 0.58rem 0.65rem;
    font-size: 0.9rem;
  }

  code {
    font-family: "IBM Plex Sans", sans-serif;
    background: rgba(255, 255, 255, 0.12);
    border: 1px solid rgba(255, 255, 255, 0.14);
    border-radius: 0.45rem;
    padding: 0.15rem 0.38rem;
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  @media (max-width: 760px) {
    .desk-shell {
      width: min(1040px, calc(100% - 1rem));
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
