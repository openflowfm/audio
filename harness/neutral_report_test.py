"""Synthetic presentation fixtures are not measurement evidence."""
import json
from pathlib import Path
import tempfile
import unittest
from run import report, resource_chart


class NeutralReportTests(unittest.TestCase):
    def test_graph_keeps_zero_distinct_from_unavailable(self):
        document = resource_chart('CPU', 'ms / trial', [('render', [('Offline', 0), ('Direct', None)])], 'scope')
        self.assertIn('audio[flow] MVP (ours): 0 ms / trial', document)
        self.assertIn('Unavailable', document)
        self.assertEqual(document.count('<rect '), 1)

    def test_profiles_are_normalized_and_workloads_stay_separate(self):
        stages = []
        for candidate, count in [('Offline', 4), ('Direct', 8)]:
            phase = dict(calls=count, wall_ns=count*2000000, process_cpu_ns=count*1000000,
                         allocations=count*3, requested_bytes=count*1024, frees=0,
                         live_delta_bytes=0, peak_live_bytes=1048576)
            e = dict(workload={'id': 'profiled-workload'}, candidate=candidate, completed=count,
                     phases={'render': phase}, artifact_write=phase,
                     process={'process_resources': {'max_rss_bytes': 2097152, 'input_block_operations': 0,
                         'output_block_operations': 0, 'minor_faults': count*2, 'major_faults': 0,
                         'user_seconds': count*.002, 'system_seconds': count*.001}, 'sampling': 'not requested'})
            stages.append(dict(name='profile-'+candidate, status='passed', evidence=e))
        aggregate = dict(workload='unprofiled-workload', candidate='Offline', eligible=True, repeats=12,
                         median_ns=2000000, min_ns=1000000, max_ns=3000000, durations_ns=[2000000]*12,
                         preparation_ns=[10]*12)
        study = dict(aggregates=[aggregate], timing_scope='enqueue + render + capture', preparation_scope='setup',
                     gates={'checks': [], 'literal_offline': [], 'first_direct': {'cases': []},
                            'first_fundsp': {'cases': []}, 'execution_errors': []})
        # Include both workloads in the canonical inventory.
        other = dict(aggregate, workload='profiled-workload')
        study['aggregates'].append(other)
        stages.insert(0, dict(name='study', status='passed', evidence=study))
        data = dict(status='passed', started_utc='synthetic', platform='synthetic', mode={}, stages=stages)
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root/'run.json').write_text(json.dumps(data))
            report(root)
            document = (root/'report.html').read_text()
            self.assertIn('<h2>unprofiled-workload</h2>', document)
            self.assertIn('<h2>profiled-workload</h2>', document)
            self.assertIn('Resource profiling not collected', document)
            self.assertIn('audio[flow] MVP (ours): 1 ms / trial', document)
            self.assertIn('Direct reference: 1 ms / trial', document)
            self.assertIn('Direct reference: 3 requests / trial', document)
            self.assertIn('FunDSP scalar reference', document)
            self.assertIn('Ours: time difference', document)
            self.assertIn('No comparison', document)
            self.assertNotIn('winner', document)
            self.assertNotIn('.candidate-table tbody tr:first-child', document)
            self.assertLess(document.index('<h2>profiled-workload'), document.index('Execution stages and actual test output'))


if __name__ == '__main__':
    unittest.main()
