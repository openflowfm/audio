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

    def native_run(self, directory, candidate):
        self.make_run(directory, ns=100)
        manifest=json.loads((directory/'manifest.json').read_text());manifest['candidates']=['Offline',candidate];manifest['native_gate_requirements']=[dict(candidate=candidate,workload='synthetic-test',gate_ids=['native-Loop'])]
        gates=json.loads((directory/'conformance.json').read_text())
        gates['fundsp_block' if candidate=='FundspBlock' else 'oddio']=[{'passed':True,'id':'native-Loop'}]
        gates['checks'] += [dict(c,candidate=candidate) for c in list(gates['checks'])]
        summary=json.loads((directory/'summary.json').read_text())
        summary['aggregates'].append(dict(summary['aggregates'][0],candidate=candidate))
        trial=json.loads((directory/'timings.jsonl').read_text())
        for name,value in [('manifest.json',manifest),('conformance.json',gates),('summary.json',summary)]:
            (directory/name).write_text(json.dumps(value))
        (directory/'timings.jsonl').write_text(json.dumps(trial)+'\n'+json.dumps(dict(trial,candidate=candidate,order=1))+'\n')
        self.rebind(directory)

    def rebind(self, directory):
        marker=json.loads((directory/'complete.json').read_text())
        marker['artifacts']=[[name,len((directory/name).read_bytes()),checksum((directory/name).read_bytes())] for name,_,_ in marker['artifacts']]
        (directory/'complete.json').write_text(json.dumps(marker))

    def test_native_references_require_own_literal_gates_and_complete_coverage(self):
        for candidate,key in [('FundspBlock','fundsp_block'),('Oddio','oddio')]:
            with tempfile.TemporaryDirectory() as root:
                directory=Path(root);self.native_run(directory,candidate)
                self.assertEqual(len(read_study(directory)[4]),2)
                gates=json.loads((directory/'conformance.json').read_text());gates[key][0]['passed']=False
                (directory/'conformance.json').write_text(json.dumps(gates));self.rebind(directory)
                with self.assertRaisesRegex(ValueError,'correctness gates'):read_study(directory)
                self.native_run(directory,candidate)
                summary=json.loads((directory/'summary.json').read_text());summary['aggregates'].pop()
                (directory/'summary.json').write_text(json.dumps(summary));self.rebind(directory)
                with self.assertRaisesRegex(ValueError,'coverage'):read_study(directory)

    def test_required_native_mode_is_explicit_and_unrelated_failure_does_not_reject(self):
        with tempfile.TemporaryDirectory() as root:
            directory=Path(root);self.native_run(directory,'Oddio')
            gates=json.loads((directory/'conformance.json').read_text())
            gates['oddio'].append({'id':'native-Once','passed':False})
            (directory/'conformance.json').write_text(json.dumps(gates));self.rebind(directory)
            self.assertTrue(read_study(directory)[4][-1]['eligible'])
            manifest=json.loads((directory/'manifest.json').read_text());manifest['native_gate_requirements']=[]
            (directory/'manifest.json').write_text(json.dumps(manifest));self.rebind(directory)
            with self.assertRaisesRegex(ValueError,'native gate requirements'):read_study(directory)

    def test_native_reference_mixed_scope_cannot_claim_pairwise_comparison(self):
        with tempfile.TemporaryDirectory() as root:
            directory=Path(root);self.native_run(directory,'Oddio')
            trials=[json.loads(line) for line in (directory/'timings.jsonl').read_text().splitlines()]
            trials[-1]['scope']='different timing boundary'
            (directory/'timings.jsonl').write_text(''.join(json.dumps(t)+'\n' for t in trials));self.rebind(directory)
            with self.assertRaisesRegex(ValueError,'mixed timing scopes'):read_study(directory)

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
