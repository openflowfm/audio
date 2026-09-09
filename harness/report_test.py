"""Synthetic artifact-validation tests. No durations here are performance evidence."""
import json
from pathlib import Path
import tempfile
import unittest
from report import read_study, checksum


class ReportValidation(unittest.TestCase):
    def make_run(self, directory, ns=9_007_199_254_740_993, source="monotonic", total=None):
        trial = {"workload":"synthetic-test","candidate":"Offline","elapsed_ns":ns,"frames":10,"source":source,"conformance":"pass"}
        objects = {
            "manifest.json":{"schema_version":1,"kind":"offline-study"},
            "conformance.json":{"schema_version":1},"workloads.json":[],
            "summary.json":{"schema_version":1,"aggregates":[{"workload":"synthetic-test","candidate":"Offline","eligible":True,"total_ns":ns if total is None else total,"total_frames":10,"min_ns":ns,"max_ns":ns,"median_ns":ns,"durations_ns":[ns]}]},
        }
        artifacts = []
        for name, value in objects.items():
            data = json.dumps(value).encode()
            (directory/name).write_bytes(data)
            artifacts.append([name,len(data),checksum(data)])
        data = (json.dumps(trial)+"\n").encode()
        (directory/"timings.jsonl").write_bytes(data)
        artifacts.append(["timings.jsonl",len(data),checksum(data)])
        (directory/"complete.json").write_text(json.dumps({"schema_version":1,"artifacts":artifacts}))

    def test_large_integer_survives_reader_without_float_rounding(self):
        with tempfile.TemporaryDirectory() as root:
            directory=Path(root)
            self.make_run(directory)
            self.assertEqual(read_study(directory)[3][0]["elapsed_ns"],9_007_199_254_740_993)

    def test_bad_summary_synthetic_claim_and_incomplete_run_are_rejected(self):
        for options in ({"total":10},{"source":"synthetic-test"}):
            with tempfile.TemporaryDirectory() as root:
                directory=Path(root)
                self.make_run(directory,**options)
                with self.assertRaises(ValueError):read_study(directory)
        with tempfile.TemporaryDirectory() as root:
            directory=Path(root)
            self.make_run(directory)
            (directory/"summary.json").write_text("{}")
            with self.assertRaises(ValueError):read_study(directory)


if __name__ == "__main__":
    unittest.main()
