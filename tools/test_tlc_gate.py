"""Failure-mode tests for tools/tlc_gate.py (#28). Each test builds a
fixture registry in a temporary directory and drives the gate with a fake
TLC runner; none needs Java, cargo or the real models.

Run: python3 -m unittest discover -s tools -p 'test_*.py'
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

import tlc_gate as g

OK = g.Outcome(0, "Model checking completed. No error has been found.\n1 states generated, 1 distinct states found")
CFG = "SPECIFICATION Spec\nCONSTANTS N = 2\nINVARIANTS TypeOK Safe\n"
REGISTRY = """
[[model]]
module = "M"
dir = "models"
config = "M.cfg"
invariants = ["TypeOK", "Safe"]
status = "design-probe"
blocked_on = "#0"

[[model.mutant]]
config = "M-bad.cfg"
violates = "Safe"
"""


def violation(name: str) -> g.Outcome:
    return g.Outcome(12, f"Error: Invariant {name} is violated.\nState 1: <Initial predicate>")


def fake(results: dict[str, g.Outcome]) -> g.Runner:
    return lambda _directory, _module, config, *_timeout: results[config]


GOOD = fake({"M.cfg": OK, "M-bad.cfg": violation("Safe")})


class Fixture(unittest.TestCase):
    def setUp(self) -> None:
        self.scratch = tempfile.TemporaryDirectory()
        self.root = Path(self.scratch.name)
        (self.root / "models").mkdir()
        (self.root / "docs").mkdir()
        (self.root / "models/M.tla").write_text("---- MODULE M ----\n====\n")
        (self.root / "models/M.cfg").write_text(CFG)
        (self.root / "models/M-bad.cfg").write_text(CFG)
        (self.root / "docs/TAUTEN.md").write_text(f"x\n{g.DOCS_BEGIN}\n{g.DOCS_END}\ny\n")
        self.write(REGISTRY)

    def tearDown(self) -> None:
        self.scratch.cleanup()

    def write(self, registry: str, update: bool = True) -> None:
        (self.root / g.REGISTRY).write_text(registry)
        if update:
            g.update(self.root)

    def run_gate(self, runner: g.Runner = GOOD) -> dict:
        return g.gate(self.root, runner, check_symbols=False)


class GateTests(Fixture):
    def test_good_registry_passes(self) -> None:
        report = self.run_gate()
        self.assertTrue(report["passed"], report["errors"])
        self.assertEqual(len(report["cases"]), 2)

    def test_failing_positive_config_fails(self) -> None:
        report = self.run_gate(fake({"M.cfg": violation("Safe"), "M-bad.cfg": violation("Safe")}))
        self.assertFalse(report["passed"])

    def test_passing_mutant_fails(self) -> None:
        report = self.run_gate(fake({"M.cfg": OK, "M-bad.cfg": OK}))
        self.assertFalse(report["passed"])

    def test_mutant_violating_another_invariant_fails(self) -> None:
        report = self.run_gate(fake({"M.cfg": OK, "M-bad.cfg": violation("TypeOK")}))
        self.assertFalse(report["passed"])

    def test_unparsable_output_fails(self) -> None:
        garbage = g.Outcome(0, "Exception in thread main")
        report = self.run_gate(fake({"M.cfg": garbage, "M-bad.cfg": g.Outcome(1, "Parse error")}))
        self.assertFalse(report["passed"])
        self.assertEqual(sum(not case["passed"] for case in report["cases"]), 2)

    def test_timeout_is_a_failure(self) -> None:
        def times_out(*_args: object) -> g.Outcome:
            raise g.GateError("M.cfg: timed out")
        with self.assertRaises(g.GateError):
            self.run_gate(times_out)

    def test_zero_models_fails(self) -> None:
        (self.root / g.REGISTRY).write_text("# nothing\n")
        with self.assertRaises(g.GateError):
            self.run_gate()

    def test_stale_snapshot_fails(self) -> None:
        (self.root / "models/M.cfg").write_text(CFG + "\\* edited\n")
        report = self.run_gate()
        self.assertFalse(report["passed"])
        self.assertIn("stale", " ".join(report["errors"]))

    def test_stale_docs_table_fails(self) -> None:
        docs = self.root / "docs/TAUTEN.md"
        docs.write_text(f"x\n{g.DOCS_BEGIN}\n{g.DOCS_END}\n")
        report = self.run_gate()
        self.assertFalse(report["passed"])

    def test_registered_invariant_must_be_checked_by_the_config(self) -> None:
        with self.assertRaises(g.GateError):
            self.write(REGISTRY.replace('"TypeOK", "Safe"', '"TypeOK", "Safe", "Live"'))

    def test_mapped_model_needs_symbols_and_tests(self) -> None:
        with self.assertRaises(g.GateError):
            self.write(REGISTRY.replace('status = "design-probe"', 'status = "mapped"'))


TEMPORAL = (REGISTRY + """
[[model.mutant]]
config = "M-starve.cfg"
violates = "Live"
""").replace('invariants = ["TypeOK", "Safe"]', 'invariants = ["TypeOK", "Safe"]\nproperties = ["Live"]')
LIVE_CFG = CFG + "PROPERTY Live\n"
TEMPORAL_VIOLATION = g.Outcome(13, "Error: Temporal properties were violated.\nState 1: <Initial predicate>")


class PropertyTests(Fixture):
    def setUp(self) -> None:
        super().setUp()
        for name in ("M.cfg", "M-bad.cfg", "M-starve.cfg"):
            (self.root / "models" / name).write_text(LIVE_CFG)
        self.write(TEMPORAL)

    def runner(self, starve: g.Outcome) -> g.Runner:
        return fake({"M.cfg": OK, "M-bad.cfg": violation("Safe"), "M-starve.cfg": starve})

    def test_temporal_mutant_must_violate_a_property(self) -> None:
        report = self.run_gate(self.runner(TEMPORAL_VIOLATION))
        self.assertTrue(report["passed"], report["errors"])

    def test_temporal_mutant_that_passes_fails(self) -> None:
        self.assertFalse(self.run_gate(self.runner(OK))["passed"])

    def test_temporal_mutant_failing_an_invariant_instead_fails(self) -> None:
        self.assertFalse(self.run_gate(self.runner(violation("Safe")))["passed"])

    def test_property_must_be_checked_by_the_config(self) -> None:
        (self.root / "models/M.cfg").write_text(CFG)
        with self.assertRaises(g.GateError):
            self.run_gate(self.runner(TEMPORAL_VIOLATION))


class RatchetTests(Fixture):
    def test_update_refuses_to_drop_a_mutant(self) -> None:
        weaker = REGISTRY.split("[[model.mutant]]")[0]
        with self.assertRaises(g.GateError):
            self.write(weaker)

    def test_amend_allows_a_reasoned_weakening(self) -> None:
        weaker = REGISTRY.split("[[model.mutant]]")[0] + '\n[[amend]]\nmodule = "M"\nreason = "mutant moved"\n'
        self.write(weaker)

    def test_hand_edited_weakening_fails_the_gate(self) -> None:
        self.write(REGISTRY.split("[[model.mutant]]")[0], update=False)
        report = self.run_gate(fake({"M.cfg": OK}))
        self.assertTrue(any("lost" in error for error in report["errors"]), report["errors"])


class ToolTests(unittest.TestCase):
    def test_missing_tool_is_blocked(self) -> None:
        with mock.patch.dict(os.environ, {}, clear=True), self.assertRaises(g.GateError):
            g.jar_from_env()

    def test_wrong_digest_is_refused(self) -> None:
        with tempfile.NamedTemporaryFile() as jar:
            jar.write(b"not tlc")
            jar.flush()
            with mock.patch.dict(os.environ, {g.TOOL_ENV: jar.name}), self.assertRaises(g.GateError):
                g.jar_from_env()


class MappingTests(unittest.TestCase):
    def setUp(self) -> None:
        self.scratch = tempfile.TemporaryDirectory()
        src = Path(self.scratch.name) / "src"
        (src / "store").mkdir(parents=True)
        (src / "lib.rs").write_text("pub mod store;\npub fn top() {}\n")
        (src / "store.rs").write_text("pub struct Store;\nimpl Store {\n    pub fn apply(&self) {}\n}\n")
        (src / "store/admit.rs").write_text("pub fn admit() {}\n")
        self.crates = {"demo": src}

    def tearDown(self) -> None:
        self.scratch.cleanup()

    def data(self, symbols: list[str], tests: list[str]) -> dict:
        return {"model": [{"module": "M", "production_symbols": symbols, "runtime_tests": tests}]}

    def check(self, symbols: list[str], tests: list[str] = []) -> list[str]:
        lister = lambda _root, package: {"store::tests::acks"} if package == "demo" else set()
        return g.check_mapping(self.data(symbols, tests), Path(self.scratch.name), self.crates, lister)

    def test_existing_symbols_resolve(self) -> None:
        self.assertEqual(self.check(["demo::top", "demo::store::{Store, admit::admit}", "demo::store::Store::apply"]), [])

    def test_unknown_symbol_fails(self) -> None:
        self.assertEqual(len(self.check(["demo::store::missing", "demo::store::Store::gone", "other::top"])), 3)

    def test_runtime_tests_resolve_or_fail(self) -> None:
        self.assertEqual(self.check([], ["demo::store::tests::acks"]), [])
        self.assertEqual(len(self.check([], ["demo::store::tests::nope"])), 1)


if __name__ == "__main__":
    unittest.main()
