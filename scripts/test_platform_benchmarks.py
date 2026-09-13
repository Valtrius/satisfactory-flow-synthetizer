"""Preparation guards; these tests do not build or run any solver."""
import copy
import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('platform_prep', ROOT / 'scripts/prepare-platform-benchmarks.py')
prep = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prep)


class SuiteGuards(unittest.TestCase):
    def setUp(self):
        self.suite = json.loads((ROOT / 'benchmarks/platform/suite.json').read_text())

    def test_retained_suite_is_valid(self):
        prep.validate_suite(self.suite)

    def test_unsafe_or_unbalanced_budgets_are_rejected(self):
        for key, value in [('session_seconds', 10801), ('repeats', 1), ('repeats', 3), ('cleanup_seconds', 0)]:
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                prep.validate_suite({**self.suite, key: value})

    def test_case_paths_duplicates_and_small_worker_counts_are_rejected(self):
        for key, value in [('case', '../outside'), ('workers', [4]), ('workers', [8, 8]), ('mode', 'unknown')]:
            suite = copy.deepcopy(self.suite)
            suite['cases'][0][key] = value
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                prep.validate_suite(suite)

    def test_triplet_must_leave_time_for_cleanup_and_verification(self):
        suite = copy.deepcopy(self.suite)
        suite['cases'][0]['seconds'] = suite['session_seconds']
        with self.assertRaises(ValueError):
            prep.validate_suite(suite)


if __name__ == '__main__':
    unittest.main()
