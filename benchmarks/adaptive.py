#!/usr/bin/env python3
"""
Adaptive load controller for k6.
Ramps concurrent users until response time exceeds 1.5s or error rate is too high.

Uses direct probe requests each tick instead of k6's lifetime-aggregate percentiles,
so the stop condition reflects current system state, not historical averages.
"""

import argparse
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request

K6_API = "http://localhost:6565/v1"
LATENCY_LIMIT_MS = 1500
ERROR_THRESHOLD = 0.02


def k6_get(path):
    try:
        req = urllib.request.Request(f"{K6_API}{path}")
        with urllib.request.urlopen(req, timeout=3) as resp:
            return json.loads(resp.read())
    except Exception:
        return None


def k6_patch_vus(n):
    data = json.dumps({"data": {"attributes": {"vus": n}}}).encode()
    req = urllib.request.Request(
        f"{K6_API}/status", data=data,
        headers={"Content-Type": "application/json"},
        method="PATCH",
    )
    try:
        urllib.request.urlopen(req, timeout=3)
    except Exception as e:
        print(f"  failed to set VUs: {e}")


def parse_metrics(raw):
    out = {"reqs": 0, "err_rate": 0.0}
    if not raw or "data" not in raw:
        return out
    for m in raw["data"]:
        mid = m.get("id", "")
        sample = m.get("attributes", {}).get("sample", {})
        if mid == "http_reqs":
            out["reqs"] = int(sample.get("count", 0))
        elif mid == "http_req_failed":
            out["err_rate"] = sample.get("rate", 0.0)
    return out


def probe_latency(base_url, token, user_id, counter_id):
    """Make a real RPC call and measure response time."""
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    data = json.dumps({"args": {"user_id": user_id, "id": counter_id}}).encode()
    req = urllib.request.Request(
        f"{base_url}/_api/rpc/get_counter", data=data, headers=headers
    )
    t0 = time.monotonic()
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            resp.read()
        return (time.monotonic() - t0) * 1000
    except Exception:
        return None


def setup_probe_data(base_url):
    """Create a user and counter for probe requests."""
    headers = {"Content-Type": "application/json"}

    data = json.dumps({"args": {"name": f"probe-{time.time_ns()}"}}).encode()
    req = urllib.request.Request(f"{base_url}/_api/rpc/register", data=data, headers=headers)
    with urllib.request.urlopen(req, timeout=10) as resp:
        body = json.loads(resp.read())
    token = body["data"]["token"]
    user_id = body["data"]["user_id"]

    data = json.dumps({"args": {"name": f"probe-counter"}}).encode()
    req = urllib.request.Request(f"{base_url}/_api/rpc/create_counter", data=data, headers=headers)
    with urllib.request.urlopen(req, timeout=10) as resp:
        body = json.loads(resp.read())
    counter_id = body["data"]["id"]

    return token, user_id, counter_id


def run(tick, ramp_step, max_vus, base_url):
    vus = 10
    prev_reqs = None
    prev_time = None
    peak_rps = 0.0

    token, user_id, counter_id = setup_probe_data(base_url)

    k6_patch_vus(vus)
    print(f"  starting at {vus} concurrent users\n")

    while True:
        time.sleep(tick)

        status = k6_get("/status")
        if not status:
            print("  k6 not responding, test ended")
            break
        if not status.get("data", {}).get("attributes", {}).get("running", False):
            print("  k6 test finished")
            break

        raw = k6_get("/metrics")
        m = parse_metrics(raw)
        now = time.monotonic()

        if prev_reqs is None:
            if m["reqs"] > 0:
                prev_reqs = m["reqs"]
                prev_time = now
                print(f"  warmup complete ({m['reqs']} setup reqs)")
            else:
                print("  (warming up...)")
            continue

        d_reqs = m["reqs"] - prev_reqs
        dt = now - prev_time
        if d_reqs <= 0 or dt <= 0:
            continue

        rps = d_reqs / dt
        if rps > vus * 15:
            prev_reqs = m["reqs"]
            prev_time = now
            continue

        prev_reqs = m["reqs"]
        prev_time = now
        peak_rps = max(peak_rps, rps)

        # Probe the actual current latency under load
        latency = probe_latency(base_url, token, user_id, counter_id)
        err = m["err_rate"]

        if latency is None:
            print(f"  {vus:>5} users | {rps:>7.0f} req/s | probe=timeout | err={err:.2%}")
            print(f"\n  stopped: probe request timed out")
            print(f"  concurrent users: {vus}")
            print(f"  peak throughput: {peak_rps:.0f} req/s")
            return vus, peak_rps

        print(f"  {vus:>5} users | {rps:>7.0f} req/s | latency={latency:>7.0f}ms | err={err:.2%}")

        if latency > LATENCY_LIMIT_MS or err > ERROR_THRESHOLD:
            reason = f"latency={latency:.0f}ms" if latency > LATENCY_LIMIT_MS else f"err={err:.2%}"
            print(f"\n  stopped: {reason}")
            print(f"  concurrent users: {vus}")
            print(f"  peak throughput: {peak_rps:.0f} req/s")
            return vus, peak_rps

        vus = min(vus + ramp_step, max_vus)
        k6_patch_vus(vus)

        if vus >= max_vus:
            print(f"\n  hit max ({max_vus}) without breaking")
            print(f"  peak throughput: {peak_rps:.0f} req/s")
            return vus, peak_rps

    return vus, peak_rps


def main():
    parser = argparse.ArgumentParser(description="Adaptive load controller for k6")
    parser.add_argument("--tick", type=int, default=5)
    parser.add_argument("--ramp-step", type=int, default=50)
    parser.add_argument("--max-vus", type=int, default=10000)
    parser.add_argument("--duration", default="10m")
    parser.add_argument("--url", default="http://localhost:8080")
    parser.add_argument("--k6-script", default=None)
    args = parser.parse_args()

    script_dir = os.path.dirname(os.path.abspath(__file__))
    k6_script = args.k6_script or os.path.join(script_dir, "app", "bench.js")

    try:
        req = urllib.request.Request(f"{args.url}/_api/health")
        urllib.request.urlopen(req, timeout=5)
    except Exception:
        print(f"App not reachable at {args.url}/_api/health")
        sys.exit(1)

    print(f"Target: {args.url}")
    print(f"Ramp: +{args.ramp_step} users every {args.tick}s, max {args.max_vus}")
    print(f"Stop: latency > {LATENCY_LIMIT_MS}ms or err > {ERROR_THRESHOLD:.0%}\n")

    k6_proc = subprocess.Popen(
        [
            "k6", "run", k6_script,
            "-e", f"BASE_URL={args.url}",
            "-e", f"DURATION={args.duration}",
            "-e", f"MAX_VUS={args.max_vus}",
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )

    for _ in range(30):
        if k6_get("/status"):
            break
        time.sleep(1)
    else:
        print("k6 API never became available")
        k6_proc.terminate()
        sys.exit(1)

    try:
        vus, peak_rps = run(args.tick, args.ramp_step, args.max_vus, args.url)
    except KeyboardInterrupt:
        print("\ninterrupted")

    try:
        k6_proc.communicate(timeout=30)
    except subprocess.TimeoutExpired:
        k6_proc.terminate()


if __name__ == "__main__":
    main()
