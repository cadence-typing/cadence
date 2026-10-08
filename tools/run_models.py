#!/usr/bin/env python3
"""Check the handover's bounded TLA models, not the future implementation."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_TOOL_SHA256 = "936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88"
TOOL_ENV = "TLA2TOOLS_JAR"
MODEL_TIMEOUT_SECONDS = 60
CASES = [
    ("AsyncAdmission", "AsyncAdmission.cfg", None),
    ("DurableCheckpoint", "DurableCheckpoint.cfg", None),
    ("CausalBarrier", "CausalBarrier.cfg", None),
    ("AsyncAdmission", "AsyncAdmission-no-epoch.cfg", "AcceptedFresh"),
    ("AsyncAdmission", "AsyncAdmission-no-pending.cfg", "AtMostOnce"),
    ("DurableCheckpoint", "DurableCheckpoint-early-ack.cfg", "AckIsDurable"),
    ("DurableCheckpoint", "DurableCheckpoint-double-credit.cfg", "NoDoubleCredit"),
    ("CausalBarrier", "CausalBarrier-early-return.cfg", "NoEarlyReturn"),
]


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    supplied = os.environ.get(TOOL_ENV)
    if not supplied:
        raise SystemExit(f"Set {TOOL_ENV} to the checksum-verified TLC 1.7.4 jar")
    jar = Path(supplied).resolve(strict=True)
    if sha256(jar) != EXPECTED_TOOL_SHA256:
        raise SystemExit("TLC tool digest mismatch; no checks executed")
    output = ROOT / "evidence" / "model-checks"
    output.mkdir(parents=True, exist_ok=True)
    results = []
    for module, config, expected_fault in CASES:
        with tempfile.TemporaryDirectory(prefix="cadence-tlc-") as temporary:
            command = ["java", "-XX:+UseParallelGC", "-Xmx256m", "-cp", str(jar),
                       "tlc2.TLC", "-workers", "1", "-config", config,
                       "-metadir", temporary, module]
            try:
                run = subprocess.run(command, cwd=ROOT / "formal", text=True,
                                     stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                     timeout=MODEL_TIMEOUT_SECONDS, check=False)
            except subprocess.TimeoutExpired as error:
                raise SystemExit(f"{config}: timed out; this is not a fault kill") from error
        log = run.stdout
        (output / f"{config}.log").write_text(log, encoding="utf-8")
        if expected_fault is None:
            passed = run.returncode == 0 and "No error has been found" in log
        else:
            passed = (run.returncode != 0
                      and f"Invariant {expected_fault} is violated" in log
                      and "State 1:" in log)
        counts = re.search(r"(\d+) states generated, (\d+) distinct states found", log)
        result = {"module": module, "config": config, "expected_fault": expected_fault,
                  "passed": passed, "return_code": run.returncode,
                  "model_sha256": sha256(ROOT / "formal" / f"{module}.tla"),
                  "config_sha256": sha256(ROOT / "formal" / config),
                  "log_sha256": sha256(output / f"{config}.log"),
                  "states_generated": int(counts[1]) if counts else None,
                  "distinct_states": int(counts[2]) if counts else None}
        results.append(result)
        print(config, "PASS" if passed else "FAIL", result["distinct_states"])
        if not passed:
            print(log[-7000:])
            break
    report = {"scope": "bounded abstract design checks only",
              "tool": "TLC 1.7.4", "tool_sha256": EXPECTED_TOOL_SHA256,
              "all_passed": len(results) == len(CASES) and all(r["passed"] for r in results),
              "cases": results}
    (ROOT / "evidence" / "tla-results.json").write_text(json.dumps(report, indent=2) + "\n")
    if not report["all_passed"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
