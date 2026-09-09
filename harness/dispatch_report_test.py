"""Synthetic validator fixtures; these are not experimental performance evidence."""
import json
from pathlib import Path
import tempfile
import unittest
from report import checksum
from dispatch_report import read_experiment, summaries


class DispatchEvidenceTests(unittest.TestCase):
    def fixture(self):
        trials = []
        for number in range(2):
            for strategy in ('Generic', 'FlatFused'):
                trials.append(dict(replicate=0,strategy=strategy,trial=dict(workload='synthetic',candidate='Offline',source='monotonic',conformance='pass',frames=32,sample_count=32,block=256,elapsed_ns=100+number,preparation_ns=20,order=len(trials),trial=number,scope='test-only')))
        return dict(schema_version=1,kind='dispatch-experiment',passed=True,errors=[],gates=[{'gates':[{'passed':True}]}],checks=[{'check':{'outcome':'pass'}}],replicates=1,repeats_per_replicate=2,workloads=[{'id':'synthetic','frames':32}],trials=trials)

    def write(self, root, data):
        raw=json.dumps(data).encode();(root/'experiment.json').write_bytes(raw)
        (root/'complete.json').write_text(json.dumps(dict(schema_version=1,artifacts=[['experiment.json',len(raw),checksum(raw)]])))

    def test_same_workload_strategy_repeats_are_required(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);data=self.fixture();self.write(root,data)
            result=read_experiment(root);self.assertEqual(summaries(result)[0]['fused_over_generic'],1)
            data['trials'].pop();self.write(root,data)
            with self.assertRaisesRegex(ValueError,'repeats'):read_experiment(root)

    def test_counted_strategy_requires_full_groups_and_matching_scope(self):
        import copy
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); data=self.fixture()
            for record in list(data['trials']):
                if record['strategy'] == 'FlatFused':
                    counted=copy.deepcopy(record); counted['strategy']='FlatFusedCounted'
                    counted['trial']['order']=len(data['trials']); data['trials'].append(counted)
            self.write(root,data)
            self.assertIn('FlatFusedCounted', summaries(read_experiment(root))[0])
            data['trials'][-1]['trial']['scope']='different'
            self.write(root,data)
            with self.assertRaisesRegex(ValueError,'scope'):read_experiment(root)
            data['trials'].pop();self.write(root,data)
            with self.assertRaisesRegex(ValueError,'repeats'):read_experiment(root)

    def test_failed_gate_or_corrupted_artifact_cannot_claim_comparison(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);data=self.fixture();data['checks'][0]['check']['outcome']='fail';self.write(root,data)
            with self.assertRaisesRegex(ValueError,'gates'):read_experiment(root)
            data['passed']=False;self.write(root,data);self.assertEqual(summaries(read_experiment(root)),[])
            (root/'experiment.json').write_text('{}')
            with self.assertRaisesRegex(ValueError,'modified'):read_experiment(root)


if __name__=='__main__':unittest.main()
