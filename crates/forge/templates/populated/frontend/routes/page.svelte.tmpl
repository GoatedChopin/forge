<script lang="ts">
  import {
    createUser,
    updateUser,
    deleteUser,
    trackExportUsers,
    trackAccountVerification,
    getUsers$,
    getIssLocation$,
    getTrades$,
    getWebhookEvents$,
    type User,
  } from "$lib/forge";

  const apiUrl = import.meta.env.VITE_API_URL || "http://localhost:8080";

  const users = getUsers$();
  const issLocation = getIssLocation$();
  const trades = getTrades$();
  const webhookEvents = getWebhookEvents$();

  let name = $state("");
  let email = $state("");
  let isSubmitting = $state(false);
  let selectedUser = $state<User | null>(null);

  let editingUserId = $state<string | null>(null);
  let editName = $state("");
  let editEmail = $state("");
  let isEditing = $state(false);
  let deletePopoverUserId = $state<string | null>(null);

  let exportJobStore = $state<ReturnType<typeof trackExportUsers> | null>(null);
  let workflowStore = $state<ReturnType<typeof trackAccountVerification> | null>(null);

  let idempotencyKey = $state(generateKey());
  let keyUsed = $state(false);
  let webhookError = $state<string | null>(null);

  function generateKey(): string {
    return `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  }

  async function triggerWebhook() {
    webhookError = null;
    const secret = "demo-secret";
    const payload = JSON.stringify({ action: "test", ts: Date.now() });

    // Generate HMAC-SHA256 signature
    const encoder = new TextEncoder();
    const key = await crypto.subtle.importKey(
      "raw",
      encoder.encode(secret),
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"]
    );
    const signature = await crypto.subtle.sign("HMAC", key, encoder.encode(payload));
    const signatureHex = Array.from(new Uint8Array(signature))
      .map((b) => b.toString(16).padStart(2, "0"))
      .join("");

    const res = await fetch(`${apiUrl}/_api/webhooks/demo`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Webhook-Signature": signatureHex,
        "X-Idempotency-Key": idempotencyKey,
      },
      body: payload,
    });

    if (res.ok) {
      keyUsed = true;
    } else {
      webhookError = `Error: ${res.status}`;
    }
  }

  function newKey() {
    idempotencyKey = generateKey();
    keyUsed = false;
    webhookError = null;
  }

  async function handleCreateUser(e: Event) {
    e.preventDefault();
    if (!name || !email) return;
    isSubmitting = true;
    await createUser({ name, email, role: null });
    name = "";
    email = "";
    isSubmitting = false;
  }

  function startEditUser(user: User) {
    editingUserId = user.id;
    editName = user.name;
    editEmail = user.email;
    selectedUser = user;
  }

  async function handleUpdateUser(e: Event) {
    e.preventDefault();
    if (!editingUserId) return;
    isEditing = true;
    await updateUser({ id: editingUserId, name: editName, email: editEmail, role: null });
    editingUserId = null;
    isEditing = false;
  }

  function cancelEdit() {
    editingUserId = null;
  }

  async function confirmDeleteUser(id: string) {
    deletePopoverUserId = null;
    await deleteUser({ id });
    if (selectedUser?.id === id) selectedUser = null;
  }

  function startExportJob() {
    exportJobStore = trackExportUsers({ format: "csv" });
  }

  function startVerificationWorkflow() {
    workflowStore = trackAccountVerification({
      user_id: selectedUser?.id || "demo-user",
      email: selectedUser?.email || "demo@example.com",
    });
  }

  function formatCoord(value: number, isLat: boolean): string {
    const dir = isLat ? (value >= 0 ? "N" : "S") : (value >= 0 ? "E" : "W");
    return `${Math.abs(value).toFixed(4)} ${dir}`;
  }

  function formatTime(ts: string): string {
    return ts ? new Date(ts).toLocaleTimeString() : "-";
  }

  function formatPrice(price: number): string {
    return price.toLocaleString(undefined, { minimumFractionDigits: 5, maximumFractionDigits: 5 });
  }

  function stepIcon(status: string): string {
    if (status === "completed") return "[=]";
    if (status === "running") return "[>]";
    if (status === "failed") return "[x]";
    return "[ ]";
  }
</script>

<main>
  <h1>Forge Demo</h1>

  <div class="grid">
    <!-- Left column: ISS, Export, Verification -->
    <div class="stack">
      <section class="card dark">
        <h2>ISS Location <span class="badge">cron</span></h2>
        {#if issLocation.data}
          <div class="stats">
            <div><span class="label">Lat</span><span class="value">{formatCoord(issLocation.data.latitude, true)}</span></div>
            <div><span class="label">Lon</span><span class="value">{formatCoord(issLocation.data.longitude, false)}</span></div>
            <div><span class="label">Time</span><span class="value">{formatTime(issLocation.data.api_timestamp)}</span></div>
          </div>
          <p class="muted small">Updated every minute via cron</p>
        {:else}
          <div class="stats">
            <div><span class="label">Lat</span><span class="value placeholder">---.----</span></div>
            <div><span class="label">Lon</span><span class="value placeholder">---.----</span></div>
            <div><span class="label">Time</span><span class="value placeholder">--:--:--</span></div>
          </div>
          <p class="muted small">Waiting for first cron run...</p>
        {/if}
      </section>

      <section class="card">
        <h2>Export Job <span class="badge">job</span></h2>
        {#if exportJobStore && $exportJobStore}
          <div class="progress-bar"><div class="fill" style="width:{$exportJobStore.progress || 0}%"></div></div>
          <p class="progress-text">{$exportJobStore.progress || 0}% - {$exportJobStore.message || $exportJobStore.status}</p>
          {#if ["completed", "failed", "pending"].includes($exportJobStore.status)}
            <button onclick={startExportJob}>Run Again</button>
          {/if}
        {:else}
          <p class="muted small export-desc">Ready to export users to CSV</p>
          <button onclick={startExportJob}>Start Export</button>
        {/if}
      </section>

      <section class="card">
        <h2>Verification <span class="badge purple">workflow</span></h2>
        {#if workflowStore && $workflowStore}
          <div class="steps">
            {#each $workflowStore.steps as step (step.name)}
              <div class="step {step.status}">
                <span class="icon">{stepIcon(step.status)}</span>
                <span>{step.name}</span>
              </div>
            {/each}
          </div>
          {#if ["completed", "failed", "compensated"].includes($workflowStore.status)}
            <button onclick={startVerificationWorkflow}>Run Again</button>
          {/if}
        {:else}
          <p class="muted small workflow-desc">Multi-step workflow with durable sleep</p>
          <button onclick={startVerificationWorkflow}>Start Workflow</button>
        {/if}
      </section>
    </div>

    <!-- Right column: Trades, Webhook -->
    <div class="stack">
      <section class="card">
        <h2>Live Trades <span class="badge green">daemon + websocket</span></h2>
        {#if trades.data && trades.data.length > 0}
          <table class="trades-table">
            <thead>
              <tr><th>Symbol</th><th>Price</th><th>Qty</th><th>Side</th></tr>
            </thead>
            <tbody>
              {#each trades.data as trade, i (trade.id || i)}
                <tr>
                  <td class="mono">{trade.symbol}</td>
                  <td class="mono">{formatPrice(trade.price)}</td>
                  <td class="mono">{trade.quantity.toFixed(4)}</td>
                  <td class={trade.is_buyer_maker ? "sell" : "buy"}>{trade.is_buyer_maker ? "SELL" : "BUY"}</td>
                </tr>
              {/each}
            </tbody>
          </table>
          <p class="muted small">Streaming from Binance EUR/USDT</p>
        {:else}
          <table class="trades-table">
            <thead>
              <tr><th>Symbol</th><th>Price</th><th>Qty</th><th>Side</th></tr>
            </thead>
            <tbody>
              <tr class="placeholder-row"><td class="mono">---</td><td class="mono">-.-----</td><td class="mono">-.----</td><td>-</td></tr>
              <tr class="placeholder-row"><td class="mono">---</td><td class="mono">-.-----</td><td class="mono">-.----</td><td>-</td></tr>
              <tr class="placeholder-row"><td class="mono">---</td><td class="mono">-.-----</td><td class="mono">-.----</td><td>-</td></tr>
              <tr class="placeholder-row"><td class="mono">---</td><td class="mono">-.-----</td><td class="mono">-.----</td><td>-</td></tr>
            </tbody>
          </table>
          <p class="muted small">Connecting to Binance WebSocket...</p>
        {/if}
      </section>

      <section class="card">
        <h2>Webhook <span class="badge">webhook</span></h2>
        <label class="input-label">Idempotency Key</label>
        <div class="webhook-row">
          <input type="text" class="key-input" class:used={keyUsed} value={idempotencyKey} readonly />
          <button class="small" onclick={newKey}>New</button>
          <button onclick={triggerWebhook} disabled={keyUsed}>Send</button>
        </div>
        {#if keyUsed}
          <p class="hint success">Webhook processed. Generate a new key to send another.</p>
        {/if}
        {#if webhookError}
          <p class="hint warning">{webhookError}</p>
        {/if}
        {#if webhookEvents.data && webhookEvents.data.length > 0}
          <label class="input-label events-label">Recent Events</label>
          <div class="events">
            {#each webhookEvents.data as ev (ev.id)}
              <div class="event"><span class="mono">{ev.idempotency_key}</span><span class="time">{formatTime(ev.processed_at)}</span></div>
            {/each}
          </div>
        {/if}
      </section>
    </div>
  </div>

  <section class="card">
    <h2>Users <span class="badge green">crud + subscribe</span></h2>
    <form onsubmit={handleCreateUser}>
      <input type="text" placeholder="Name" bind:value={name} required />
      <input type="email" placeholder="Email" bind:value={email} required />
      <button type="submit" disabled={isSubmitting}>{isSubmitting ? "..." : "Create"}</button>
    </form>

    {#if users.data && users.data.length > 0}
      <table>
        <thead><tr><th>Name</th><th>Email</th><th></th></tr></thead>
        <tbody>
          {#each users.data as user (user.id)}
            {#if editingUserId === user.id}
              <tr class="editing">
                <td><input type="text" bind:value={editName} /></td>
                <td><input type="email" bind:value={editEmail} /></td>
                <td>
                  <button class="small" onclick={handleUpdateUser} disabled={isEditing}>Save</button>
                  <button class="small secondary" onclick={cancelEdit}>Cancel</button>
                </td>
              </tr>
            {:else}
              <tr>
                <td>{user.name}</td>
                <td>{user.email}</td>
                <td>
                  <button class="small" onclick={() => startEditUser(user)}>Edit</button>
                  <button class="small danger" onclick={() => deletePopoverUserId = user.id}>Delete</button>
                  {#if deletePopoverUserId === user.id}
                    <div class="popover">
                      <button class="small danger" onclick={() => confirmDeleteUser(user.id)}>Confirm</button>
                      <button class="small" onclick={() => deletePopoverUserId = null}>Cancel</button>
                    </div>
                  {/if}
                </td>
              </tr>
            {/if}
          {/each}
        </tbody>
      </table>
    {:else if !users.loading}
      <p class="muted">No users yet. Create one above.</p>
    {/if}
  </section>
</main>

<style>
  main { max-width: 56rem; margin: 0 auto; padding: 2rem; font-family: system-ui, sans-serif; }
  h1 { margin: 0 0 0.25rem; }
  h2 { margin: 0 0 0.75rem; font-size: 1rem; }
  h1 { margin-bottom: 1.5rem; }

  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 1.5rem; margin-bottom: 1.5rem; align-items: start; }
  .grid > .stack { min-height: 100%; display: flex; flex-direction: column; justify-content: space-between; }
  .stack { display: flex; flex-direction: column; gap: 1.5rem; }

  .card { background: #fff; border: 1px solid #e2e8f0; border-radius: 12px; padding: 1.5rem; box-shadow: 0 1px 3px rgba(0,0,0,0.04); }
  .card.dark { background: linear-gradient(135deg, #0f172a, #1e293b); border-color: #334155; color: #f1f5f9; }
  .card.dark .muted { color: #64748b; }

  .badge { font-size: 0.65rem; padding: 0.15rem 0.4rem; border-radius: 3px; background: #e5e5e5; color: #666; margin-left: 0.5rem; text-transform: uppercase; }
  .badge.green { background: #dcfce7; color: #166534; }
  .badge.purple { background: #ede9fe; color: #5b21b6; }

  .stats { display: flex; gap: 2rem; }
  .stats .label { font-size: 0.75rem; color: #94a3b8; display: block; }
  .stats .value { font-size: 1.1rem; font-weight: 600; color: #60a5fa; font-family: monospace; }
  .stats .value.placeholder { color: #475569; }

  .muted { color: #999; font-style: italic; margin: 0; }
  .muted.small { font-size: 0.85rem; margin-top: 0.5rem; }
  .muted.workflow-desc { margin-bottom: 0.75rem; }
  .muted.export-desc { margin-bottom: 0.75rem; }
  .mono { font-family: monospace; }
  .path { font-size: 0.85rem; color: #666; margin: 0 0 0.75rem; }

  .trades-table { width: 100%; border-collapse: collapse; font-size: 0.9rem; }
  .trades-table th { text-align: left; font-weight: 500; color: #666; padding: 0.5rem 0; border-bottom: 1px solid #eee; }
  .trades-table td { padding: 0.5rem 0; border-bottom: 1px solid #f5f5f5; }
  .trades-table .buy { color: #16a34a; font-weight: 500; }
  .trades-table .sell { color: #dc2626; font-weight: 500; }
  .trades-table .placeholder-row { color: #ccc; }

  .input-label { display: block; font-size: 0.75rem; font-weight: 500; color: #666; margin-bottom: 0.35rem; text-transform: uppercase; letter-spacing: 0.05em; }
  .events-label { margin-top: 1rem; }
  .webhook-row { display: flex; gap: 0.5rem; margin-bottom: 0.5rem; }
  .key-input { flex: 1; padding: 0.5rem 0.75rem; font-size: 0.85rem; font-family: monospace; border: 1px solid #d1d5db; border-radius: 6px; background: #fafafa; }
  .key-input.used { border-color: #16a34a; border-width: 2px; background: #f0fdf4; color: #166534; }
  .hint { font-size: 0.8rem; margin: 0 0 0.5rem; }
  .hint.warning { color: #b45309; }
  .hint.success { color: #166534; }
  .events { display: flex; flex-direction: column; gap: 0.35rem; }
  .event { display: flex; justify-content: space-between; font-size: 0.85rem; padding: 0.5rem 0.75rem; background: #f8fafc; border: 1px solid #e2e8f0; border-radius: 6px; }
  .event .time { color: #64748b; }

  .progress-bar { height: 8px; background: #e5e5e5; border-radius: 4px; overflow: hidden; margin-bottom: 0.5rem; }
  .progress-bar .fill { height: 100%; background: #0066cc; transition: width 0.3s; }
  .progress-text { font-size: 0.85rem; color: #666; margin: 0 0 0.5rem; }

  .steps { display: flex; flex-direction: column; gap: 0.35rem; margin-bottom: 0.75rem; }
  .step { display: flex; align-items: center; gap: 0.5rem; padding: 0.4rem 0.6rem; background: #f5f5f5; border-radius: 4px; font-size: 0.9rem; }
  .step.completed { background: #dcfce7; }
  .step.running { background: #dbeafe; }
  .step.failed { background: #fee2e2; }
  .step .icon { width: 1.5rem; text-align: center; font-family: monospace; font-size: 0.8rem; }

  form { display: flex; gap: 0.5rem; margin-bottom: 1rem; }
  input { flex: 1; padding: 0.5rem 0.75rem; border: 1px solid #ccc; border-radius: 4px; font-size: 0.95rem; }
  button { padding: 0.5rem 1rem; background: #0066cc; color: white; border: none; border-radius: 4px; cursor: pointer; }
  button:hover:not(:disabled) { background: #0055aa; }
  button:disabled { background: #999; cursor: not-allowed; }
  button.small { padding: 0.25rem 0.5rem; font-size: 0.85rem; }
  button.secondary { background: #666; }
  button.danger { background: #dc2626; }

  table { width: 100%; border-collapse: collapse; }
  th, td { text-align: left; padding: 0.75rem; border-bottom: 1px solid #eee; }
  th { font-weight: 600; color: #555; }
  tr.editing { background: #f0f9ff; }

  .popover { position: absolute; right: 0; top: 100%; margin-top: 4px; background: white; border: 1px solid #e0e0e0; border-radius: 6px; padding: 0.5rem; box-shadow: 0 4px 12px rgba(0,0,0,0.15); z-index: 100; display: flex; gap: 0.5rem; }
  td:last-child { position: relative; white-space: nowrap; }

  @media (max-width: 700px) {
    .grid { grid-template-columns: 1fr; }
    .grid > .stack { min-height: auto; justify-content: flex-start; }
    form { flex-wrap: wrap; }
    form input { flex: 1 1 100%; min-width: 0; }
    form button { flex: 1 1 100%; }
    table { font-size: 0.85rem; }
    th, td { padding: 0.5rem 0.25rem; }
    td:last-child { display: flex; flex-wrap: wrap; gap: 0.25rem; }
    .popover { position: fixed; left: 50%; top: 50%; transform: translate(-50%, -50%); right: auto; }
  }
</style>
