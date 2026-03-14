import http from "k6/http";
import { sleep } from "k6";
import { Rate, Trend } from "k6/metrics";

const BASE_URL = __ENV.BASE_URL || "http://localhost:8080";
const DURATION = __ENV.DURATION || "10m";
const MAX_VUS = parseInt(__ENV.MAX_VUS || "10000");

const rpcLatency = new Trend("rpc_latency", true);
const rpcErrors = new Rate("rpc_errors");

export const options = {
  scenarios: {
    adaptive: {
      executor: "externally-controlled",
      vus: 10,
      maxVUs: MAX_VUS,
      duration: DURATION,
    },
  },
};

function rpc(name, args, token) {
  const headers = { "Content-Type": "application/json" };
  if (token) headers["Authorization"] = `Bearer ${token}`;

  const res = http.post(
    `${BASE_URL}/_api/rpc/${name}`,
    JSON.stringify({ args }),
    { headers, tags: { rpc: name } }
  );

  const ok = res.status === 200;
  rpcLatency.add(res.timings.duration, { rpc: name });
  rpcErrors.add(!ok);

  if (ok) {
    try {
      return JSON.parse(res.body);
    } catch (_) {
      rpcErrors.add(true);
      return null;
    }
  }
  return null;
}

function randomString(len) {
  const chars = "abcdefghijklmnopqrstuvwxyz0123456789";
  let s = "";
  for (let i = 0; i < len; i++) {
    s += chars[Math.floor(Math.random() * chars.length)];
  }
  return s;
}

export function setup() {
  const users = [];
  for (let i = 0; i < 20; i++) {
    const reg = rpc("register", { name: `bench-${randomString(8)}` });
    if (!reg || !reg.data) continue;
    users.push({ token: reg.data.token, userId: reg.data.user_id });
  }
  if (users.length === 0) throw new Error("Setup failed: no users created");

  const counters = [];
  for (let i = 0; i < 10; i++) {
    const c = rpc("create_counter", { name: `counter-${randomString(6)}` });
    if (c && c.data) counters.push(c.data.id);
  }
  if (counters.length === 0) throw new Error("Setup failed: no counters created");

  return { users, counters };
}

export default function (data) {
  const user = data.users[Math.floor(Math.random() * data.users.length)];
  const counterId = data.counters[Math.floor(Math.random() * data.counters.length)];
  const roll = Math.random();

  if (roll < 0.40) {
    rpc("get_counter", { user_id: user.userId, id: counterId }, user.token);
  } else if (roll < 0.70) {
    rpc("list_counters", { user_id: user.userId }, user.token);
  } else {
    rpc("increment", { user_id: user.userId, id: counterId }, user.token);
  }

  sleep(0.1);
}
