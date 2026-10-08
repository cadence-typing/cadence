#!/usr/bin/env python3
"""Validate handover consistency. This does not test a Cadence implementation."""
from __future__ import annotations

from collections import Counter
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import re
import tomllib

ROOT = Path(__file__).resolve().parents[1]
REQUEST_OPERATIONS = {"hello", "key", "text", "advance", "resize", "action",
                      "barrier", "snapshot", "capture", "deliver", "quit"}
EXPECTED_CLAIMS = {*(f"VP-{n:02d}" for n in range(1, 6)),
                   *(f"VK-{n:02d}" for n in range(1, 7)), "VL-01", "VR-01"}


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(message)


def json_file(relative: str):
    return json.loads((ROOT / relative).read_text())


def check_fixtures() -> int:
    data = json_file("fixtures/scoring.json")
    for case in data["cases"]:
        target, typed, first = case["target"], [], {}
        attempts = correct_attempts = deletions = overflows = 0
        dirty = False
        for event in case["events"]:
            if event == "Backspace":
                if typed:
                    typed.pop()
                    deletions += 1
                    dirty = True
                continue
            position = len(typed)
            attempts += 1
            correct = position < len(target) and event == target[position]
            correct_attempts += int(correct)
            dirty |= not correct
            if position < len(target):
                first.setdefault(position, correct)
            else:
                overflows += 1
            typed.append(event)
        visible = sum(character == target[index]
                      for index, character in enumerate(typed) if index < len(target))
        actual = {"first_correct": sum(first.values()), "first_attempted": len(first),
                  "correct_attempts": correct_attempts, "content_attempts": attempts,
                  "deletions": deletions, "overflow_attempts": overflows,
                  "correct_visible_units": visible,
                  "clean_completion": ("".join(typed) == target and not dirty
                                       and not case["interrupted"]),
                  "measurement_uninterrupted": not case["interrupted"]}
        expected = case["expected"]
        for key, value in actual.items():
            require(value == expected[key], f"{case['id']}: {key}: {value} != {expected[key]}")
        active = case["active_microseconds"]
        ratio = Fraction(visible * 60_000_000, 5 * active) if active else None
        supplied = expected["correct_wpm_fraction"]
        require(ratio == (Fraction(*supplied) if supplied else None),
                f"{case['id']}: WPM fraction mismatch")
        if "numeric_expected" in case:
            numeric = case["numeric_expected"]
            digits = sum(c.isascii() and c.isdigit() and c == target[i]
                         for i, c in enumerate(typed) if i < len(target))
            require(digits == numeric["correct_digits"], "Numeric digit count mismatch")
            require(Fraction(digits * 60_000_000, active)
                    == Fraction(*numeric["digits_per_minute_fraction"]),
                    "Numeric throughput mismatch")
    timing = data["timing_case"]
    paused = sum(end - start for start, end in timing["pauses"])
    require(timing["elapsed_microseconds"] - paused == timing["expected_active_microseconds"],
            "Pause arithmetic mismatch")
    return len(data["cases"])


def main() -> None:
    checked = []
    for path in ROOT.rglob("*.json"):
        json.loads(path.read_text())
    defaults = tomllib.loads((ROOT / "config/defaults.toml").read_text())
    policy = tomllib.loads((ROOT / "policies/trainer-v1.toml").read_text())
    session = policy["session"]
    require(sum(session["phase_seconds"]) == session["default_budget_seconds"]
            == defaults["practice"]["duration_seconds"] == 900, "Daily budget mismatch")
    require(len(session["phase_order"]) == len(session["phase_seconds"])
            == len(session["phase_minimum_seconds"]), "Phase vector length mismatch")
    require(sum(session["phase_minimum_seconds"]) <= session["minimum_budget_seconds"],
            "Minimum session infeasible")
    counts = Counter(policy["selection"]["lane_order"])
    require(sum(counts.values()) == policy["selection"]["block_slots"], "Slot count mismatch")
    for lane in ("target", "review", "broad"):
        require(counts[lane] == policy["selection"][lane + "_slots"], "Lane quota mismatch")
    require(sum(policy["utility_weights_basis_points"].values()) == 10_000,
            "Utility weights do not sum to one")
    require(policy["benchmark"]["probe_seconds"]
            + policy["benchmark"]["remaining_prose_seconds"]
            == session["phase_seconds"][session["phase_order"].index("prose")],
            "Benchmark would extend session")
    checked.append("TOML parsing, phase allocation, quotas, weights and benchmark replacement")
    for path in [ROOT / "README.md", ROOT / "AGENTS.md", *sorted((ROOT / "docs").glob("*.md"))]:
        for link in re.findall(r"\[[^\]]+\]\(([^)]+)\)", path.read_text()):
            if "://" in link or link.startswith("#"):
                continue
            require((path.parent / link.split("#")[0]).exists(), f"Broken link: {path}: {link}")
    checked.append("Authored Markdown local links resolve")
    schema = json_file("contracts/headless-request.schema.json")
    require(set(schema["properties"]["op"]["enum"]) == REQUEST_OPERATIONS,
            "Missing headless operations")
    require({branch["properties"]["op"]["const"] for branch in schema["oneOf"]}
            == REQUEST_OPERATIONS, "Headless argument branches mismatch")
    transcript = [json.loads(line) for line in
                  (ROOT / "fixtures/headless-session.jsonl").read_text().splitlines()]
    require(transcript[0]["op"] == "hello", "Transcript must begin with hello")
    require([int(row["id"]) for row in transcript] == list(range(1, len(transcript) + 1)),
            "Transcript request IDs are not sequential")
    for row in transcript:
        require(row["op"] in REQUEST_OPERATIONS and row["v"] == 1,
                "Invalid transcript operation/version")
    checked.append("JSON syntax, protocol operation coverage and transcript envelope checks")
    count = check_fixtures()
    checked.append(f"{count} independent scoring examples and pause arithmetic")
    claims = json_file("formal/claims.json")
    require({c["id"] for c in claims["claims"]} == EXPECTED_CLAIMS, "Claim inventory mismatch")
    require(all(c["status"] == "pending" for c in claims["claims"]),
            "Packet must not claim production proof completion")
    models = json_file("evidence/tla-results.json")
    require(models["all_passed"] and len(models["cases"]) == 8, "Missing model check evidence")
    for case in models["cases"]:
        for relative, field in [(f"formal/{case['module']}.tla", "model_sha256"),
                                (f"formal/{case['config']}", "config_sha256"),
                                (f"evidence/model-checks/{case['config']}.log", "log_sha256")]:
            require(digest(ROOT / relative) == case[field], f"Changed model evidence: {relative}")
    checked.append("3 positive TLC runs, 5 named counterexamples, source/config/log digests")
    manifest_path = ROOT / "MANIFEST.sha256"
    if manifest_path.exists():
        for line in manifest_path.read_text().splitlines():
            expected, relative = line.split("  ", 1)
            require(digest(ROOT / relative) == expected, f"Manifest mismatch: {relative}")
        print("Package manifest digests: PASS")
    report = {"schema_version": 1, "scope": "handover consistency and bounded design models",
              "passed": True, "checks": checked,
              "not_claimed": ["Cadence implementation", "Rust compilation/runtime tests",
                              "Verus production proofs", "TLAPS general proofs",
                              "human efficacy", "Buildbot deployment", "signed release"],
              "model_report": "tla-results.json"}
    # Stable content: do not include run timestamps that would break the package manifest.
    (ROOT / "evidence/packet-validation.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
