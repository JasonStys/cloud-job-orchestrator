"""File: scripts/native-core-smoke.py
Purpose: Exercise the actual owned Rust/PostgreSQL composition twice using fresh synthetic state.
Functions: launch binds readiness to the child; request enforces one exact loopback origin;
main independently checks real outputs, invalid input, cancellation, retained work and restart.
Variables: capability remains private to this harness/owned child; no ordinary user profile is read.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import queue
import secrets
import subprocess
import threading
import time
import urllib.error
import urllib.request
from pathlib import Path


class NoRedirect(urllib.request.HTTPRedirectHandler):
    """Never follow a response outside the owned local service."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def require(condition, message):
    """Keep acceptance oracles active even under optimized Python."""
    if not condition:
        raise ValueError(message)


def launch(binary, resources, assets, profile, output):
    """Start one owned child and wait for its actual readiness, never an existing service."""
    capability = secrets.token_hex(32)
    errors = (output / ("startup-" + secrets.token_hex(4) + ".log")).open("xb")
    child = subprocess.Popen(
        [
            str(binary),
            "--profile",
            str(profile),
            "--postgres",
            str(resources),
            "--assets",
            str(assets),
        ],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=errors,
    )
    child.stdin.write((capability + "\n").encode("ascii"))
    child.stdin.flush()
    received = queue.Queue(maxsize=1)
    threading.Thread(
        target=lambda: received.put(child.stdout.readline(4097)), daemon=True
    ).start()
    try:
        raw = received.get(timeout=45)
        require(0 < len(raw) <= 4096, "Actual native readiness was absent or oversized")
        ready = json.loads(raw)
        require(
            ready.get("product") == "cloud-job-orchestrator"
            and ready.get("version") == "1.1.0",
            "Unexpected native product/version",
        )
        require(
            type(ready.get("port")) is int and 0 < ready["port"] < 65536,
            "Actual owned port missing",
        )
        origin = "http://127.0.0.1:" + str(ready["port"])
        opener = urllib.request.build_opener(
            urllib.request.ProxyHandler({}), NoRedirect()
        )

        def request(path, data=None, *, token=True, origin_header=None, expected=200):
            require(path.startswith(("/v1/", "/native/")), "Non-contract route refused")
            headers = {"Content-Type": "application/json"}
            if token:
                headers["x-cjo-native"] = capability
            if origin_header:
                headers["Origin"] = origin_header
            body = None if data is None else json.dumps(data).encode("utf-8")
            req = urllib.request.Request(origin + path, body, headers)
            try:
                response = opener.open(req, timeout=10)
            except urllib.error.HTTPError as error:
                response = error
            with response:
                require(response.status == expected, "Unexpected actual HTTP status")
                raw = response.read(1_048_577)
                require(len(raw) <= 1_048_576, "Actual response exceeds bound")
                return json.loads(raw) if raw.startswith((b"{", b"[")) else raw

        return child, errors, ready, request
    except BaseException:
        child.stdin.close()
        child.wait(timeout=45)
        errors.close()
        raise


def wait_for(request, identifier, state):
    """Observe actual transactional states under a fixed deadline; no assumed-delay success."""
    deadline = time.monotonic() + 12
    while time.monotonic() < deadline:
        value = request("/v1/dags/" + identifier)
        if all(row["state"] == state for row in value["jobs"]):
            return value
        time.sleep(0.05)
    raise ValueError("Actual database state did not reach the required result")


def close(child, errors):
    """Request parent EOF then require actual normal owner/database cleanup; no forced process kill."""
    if not child.stdin.closed:
        child.stdin.close()
    require(child.wait(timeout=45) == 0, "Actual owner did not finish normally")
    errors.close()


def main():
    """Use original PostgreSQL and allowlisted workers, preserving every earlier proof directory."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--postgres", type=Path, required=True)
    parser.add_argument("--assets", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.absolute()
    output.mkdir(parents=True)
    binary, postgres, assets = (
        path.absolute() for path in (args.binary, args.postgres, args.assets)
    )
    profile = output / "synthetic-profile"
    first, errors, identity, request = launch(binary, postgres, assets, profile, output)
    try:
        request("/native/status", token=False, expected=403)
        request("/native/status", origin_header="https://foreign.invalid", expected=403)
        request("/v1/dags", {"id": "bad", "jobs": []}, expected=422)
        request(
            "/v1/dags",
            {
                "id": "telemetry",
                "jobs": [
                    {"id": "prepare", "kind": "uppercase", "payload": "sensor ready"},
                    {"id": "verify", "kind": "checksum", "payload": "abc"},
                ],
                "dependencies": [{"from": "prepare", "to": "verify"}],
            },
            expected=201,
        )
        snapshot = request("/v1/dags/telemetry")
        require(
            [row["state"] for row in snapshot["jobs"]] == ["ready", "pending"],
            "Initial dependencies did not remain paused",
        )
        request("/native/workers", {"paused": False})
        completed = wait_for(request, "telemetry", "succeeded")
        results = {row["id"]: row["result"] for row in completed["jobs"]}
        require(
            results
            == {
                "prepare": "SENSOR READY",
                "verify": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            },
            "Actual worker outputs differ from independent authored oracles",
        )
        inspection = request("/v1/dags/telemetry/inspection")
        require(
            len(inspection["events"]) == 4,
            "Real claim/completion history is incomplete",
        )
        request("/native/workers", {"paused": True})
        request(
            "/v1/dags",
            {
                "id": "cancelled",
                "jobs": [{"id": "hold", "kind": "delay", "payload": "10"}],
            },
            expected=201,
        )
        request("/v1/dags/cancelled/cancel", {})
        wait_for(request, "cancelled", "cancelled")
        request(
            "/v1/dags",
            {
                "id": "drain",
                "jobs": [{"id": "retain", "kind": "delay", "payload": "2500"}],
            },
            expected=201,
        )
        request("/native/workers", {"paused": False})
        wait_for(request, "drain", "leased")
        first.stdin.close()
        require(
            first.poll() is None,
            "Owner exited before the actual retained worker finished",
        )
    finally:
        close(first, errors)
    require(
        not (profile / "postgres/postmaster.pid").exists(),
        "Owned PostgreSQL remained running",
    )
    second, errors, reopened, request = launch(
        binary, postgres, assets, profile, output
    )
    try:
        require(
            reopened["instance"] != identity["instance"],
            "An existing service was reused",
        )
        require(
            request("/v1/dags/telemetry") == completed,
            "Durable actual results changed at restart",
        )
        retained = wait_for(request, "drain", "succeeded")
        require(
            retained["jobs"][0]["result"] == "slept:2500ms",
            "Pending work was abandoned at Quit",
        )
        wait_for(request, "cancelled", "cancelled")
    finally:
        close(second, errors)
    require(
        not (profile / "postgres/postmaster.pid").exists(),
        "Reopened PostgreSQL did not stop",
    )
    receipt = {
        "schema": 1,
        "passed": True,
        "actualLaunches": 2,
        "exitCodes": [0, 0],
        "product": "cloud-job-orchestrator",
        "version": "1.1.0",
        "source": identity.get("source"),
        "sourceDirty": identity.get("source_dirty"),
        "postgres": "18.6",
        "realOutputs": results,
        "retainedWorkCompleted": True,
        "normalUserProfileOpened": False,
        "sameResultsSha256": hashlib.sha256(
            json.dumps(completed, sort_keys=True).encode()
        ).hexdigest(),
    }
    (output / "acceptance.json").write_text(
        json.dumps(receipt, indent=2) + "\n", encoding="utf-8"
    )
    print(json.dumps({"passed": True, "actualLaunches": 2, "postgres": "18.6"}))


if __name__ == "__main__":
    main()
