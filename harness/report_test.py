"""Synthetic artifact-validation tests. No durations here are performance evidence."""
import json
from pathlib import Path
import tempfile
import unittest
from report import read_study, checksum


class ReportValidation(unittest.TestCase):
    def make_run(self, directory, ns=9_007_199_254_740_993, source="monotonic", total=None, gate=True, duplicate=False):
        trial = {"schema_version":1,"workload":"synthetic-test","candidate":"Offline","trial":0,"order":0,"elapsed_ns":ns,"frames":10,"sample_count":10,"preparation_ns":3,"block":10,"scope":"synthetic-reader-test","source":source,"conformance":"pass"}
        objects = {
            "manifest.json":{"schema_version":1,"kind":"offline-study","repeats":1,"timing_block":10,"conformance_blocks":[1,64,257]},
            "conformance.json":{"schema_version":1,"checks":[{"workload":"synthetic-test","candidate":"Offline","block":block,"outcome":"pass" if gate else "fail"} for block in [1,64,257]],"literal_offline":[{"passed":True}],"first_direct":{"cases":[{"outcome":"pass"}]},"first_fundsp":{"cases":[{"outcome":"pass"}]},"execution_errors":[]},
            "workloads.json":[{"id":"synthetic-test","frames":10}],
            "summary.json":{"schema_version":1,"aggregates":[{"workload":"synthetic-test","candidate":"Offline","eligible":True,"repeats":1,"total_ns":ns if total is None else total,"total_frames":10,"min_ns":ns,"max_ns":ns,"median_ns":ns,"durations_ns":[ns],"preparation_ns":[3]}]},
        }
        artifacts = []
        for name, value in objects.items():
            data = json.dumps(value).encode()
            (directory/name).write_bytes(data)
            artifacts.append([name,len(data),checksum(data)])
        data = ((json.dumps(trial)+"\n")*(2 if duplicate else 1)).encode()
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

    def test_false_speed_eligibility_and_duplicate_trials_are_rejected(self):
        for options in ({"gate":False},{"duplicate":True}):
            with tempfile.TemporaryDirectory() as root:
                directory=Path(root)
                self.make_run(directory,**options)
                with self.assertRaises(ValueError):read_study(directory)


if __name__ == "__main__":
    unittest.main()
