const apiBase = process.env.API_URL || "http://localhost:8080";
const endpoint = `${apiBase}/_api/mcp`;
const protocolVersion = "2025-11-25";

interface JsonRpcResponse {
  result?: unknown;
  error?: { code: number; message: string; data?: unknown };
}

function ensure(condition: unknown, message: string): asserts condition {
  if (!condition) {
    throw new Error(message);
  }
}

async function postJson(
  body: Record<string, unknown>,
  headers: Record<string, string>,
): Promise<{ status: number; headers: Headers; json: JsonRpcResponse }> {
  const response = await fetch(endpoint, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      ...headers,
    },
    body: JSON.stringify(body),
  });

  const json = (await response.json()) as JsonRpcResponse;
  return { status: response.status, headers: response.headers, json };
}

async function call(
  id: number,
  method: string,
  params: Record<string, unknown>,
  headers: Record<string, string>,
): Promise<unknown> {
  const { status, json } = await postJson(
    {
      jsonrpc: "2.0",
      id,
      method,
      params,
    },
    headers,
  );

  ensure(status === 200, `Expected 200 for ${method}, got ${status}`);
  if (json.error) {
    throw new Error(
      `${method} failed with ${json.error.code}: ${json.error.message}`,
    );
  }
  return json.result;
}

function extractToolStructured(result: unknown): unknown {
  const payload = result as
    | { structuredContent?: unknown; content?: Array<{ text?: string }> }
    | undefined;
  if (!payload) {
    return undefined;
  }

  if (payload.structuredContent !== undefined) {
    return payload.structuredContent;
  }

  const text = payload.content?.[0]?.text;
  if (typeof text === "string") {
    try {
      return JSON.parse(text) as unknown;
    } catch {
      return undefined;
    }
  }

  return undefined;
}

async function main() {
  const nonce = Date.now();
  const title = `MCP outage check ${nonce}`;
  const note = `Investigated by MCP script ${nonce}`;

  const init = await postJson(
    {
      jsonrpc: "2.0",
      id: 1,
      method: "initialize",
      params: {
        protocolVersion: protocolVersion,
        capabilities: {},
        clientInfo: { name: "support-desk-mcp-test", version: "1.0.0" },
      },
    },
    {},
  );

  ensure(init.status === 200, `Initialize status was ${init.status}`);
  ensure(!init.json.error, "Initialize returned an error");

  const sessionId = init.headers.get("mcp-session-id");
  ensure(sessionId, "Missing mcp-session-id header from initialize");

  const baseHeaders = {
    "mcp-session-id": sessionId,
    "mcp-protocol-version": protocolVersion,
  };

  const initialized = await postJson(
    {
      jsonrpc: "2.0",
      method: "notifications/initialized",
      params: {},
    },
    baseHeaders,
  );
  ensure(
    initialized.status === 202,
    `Expected 202 for notifications/initialized, got ${initialized.status}`,
  );

  const toolsList = (await call(2, "tools/list", {}, baseHeaders)) as {
    tools: Array<{ name: string }>;
  };
  const toolNames = toolsList.tools.map((tool) => tool.name);

  const expectedTools = [
    "support.list_tickets",
    "support.create_ticket",
    "support.set_status",
    "support.set_priority",
    "support.add_note",
  ];
  for (const name of expectedTools) {
    ensure(toolNames.includes(name), `tools/list missing ${name}`);
  }

  const createdRaw = await call(
    3,
    "tools/call",
    {
      name: "support.create_ticket",
      arguments: {
        customer_name: "MCP Integration",
        title,
        details: "Created from MCP integration script",
        priority: "normal",
      },
    },
    baseHeaders,
  );

  const created = extractToolStructured(createdRaw) as { id?: string } | undefined;

  const ticketId = created?.id;
  ensure(ticketId, "support.create_ticket did not return ticket id");

  await call(
    4,
    "tools/call",
    {
      name: "support.set_status",
      arguments: {
        id: ticketId,
        status: "working",
      },
    },
    baseHeaders,
  );

  await call(
    5,
    "tools/call",
    {
      name: "support.set_priority",
      arguments: {
        id: ticketId,
        priority: "high",
      },
    },
    baseHeaders,
  );

  const notedRaw = await call(
    6,
    "tools/call",
    {
      name: "support.add_note",
      arguments: {
        id: ticketId,
        note,
      },
    },
    baseHeaders,
  );

  const noted = extractToolStructured(notedRaw) as { last_note?: string } | undefined;

  ensure(
    noted?.last_note === note,
    "support.add_note did not persist the expected note",
  );

  const listedRaw = await call(
    7,
    "tools/call",
    {
      name: "support.list_tickets",
      arguments: {},
    },
    baseHeaders,
  );

  const listed = extractToolStructured(listedRaw) as
    | Array<{ id: string; status: string; priority: string }>
    | undefined;

  const ticket = listed?.find((item) => item.id === ticketId);
  ensure(ticket, "support.list_tickets did not return created ticket");
  ensure(ticket.status === "working", "Ticket status is not 'working' after MCP update");
  ensure(ticket.priority === "high", "Ticket priority is not 'high' after MCP update");

  console.log("MCP integration script passed");
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
