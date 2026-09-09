"""Timestamp and coloring tests; synthetic fixtures are never benchmark evidence."""
import ctypes
import os
import time
import unittest
from run import sample_points, time_chart
from impact_charts import duration_ranks
from resource_sampling import ProcessReader, ResourceSampler, RusageV2


class TimeSeriesTests(unittest.TestCase):
    def test_all_eligible_medians_rank_with_shared_ties(self):
        self.assertEqual(duration_ranks([1, 2, 3]), ['timing-low', 'timing-middle', 'timing-high'])
        self.assertEqual(duration_ranks([1, 1, 2]), ['timing-low', 'timing-low', 'timing-high'])
        self.assertEqual(duration_ranks([1, 2, 2]), ['timing-low', 'timing-high', 'timing-high'])
        self.assertEqual(duration_ranks([2, 2, None]), ['', '', ''])
        self.assertEqual(duration_ranks([None, 1]), ['', ''])
        self.assertEqual(duration_ranks([]), [])

    def test_rates_use_actual_elapsed_intervals_and_missing_breaks_trace(self):
        samples = [dict(elapsed_ns=0, n=100), dict(elapsed_ns=500000000, n=150),
                   dict(elapsed_ns=1500000000, n=350), dict(elapsed_ns=2000000000, n=None),
                   dict(elapsed_ns=3000000000, n=400), dict(elapsed_ns=4000000000, n=1)]
        self.assertEqual(sample_points(samples, 'n', rate=True), [(0, None), (.5, 100), (1.5, 200), (2, None), (3, None), (4, None)])
        document = time_chart('rate', 'bytes/s', [('Offline', sample_points(samples, 'n', rate=True))], 'scope')
        self.assertEqual(document.count('<circle '), 2)
        self.assertNotIn('ours-better', document)
        self.assertNotIn('ours-slower', document)
        self.assertIn('1.500000 s, 200 bytes/s', document)
        self.assertIn('Unavailable', time_chart('no data', 'bytes', [('Offline', [])], 'scope'))

    def test_real_process_observations_have_monotonic_times_and_scope(self):
        sampler = ResourceSampler(os.getpid(), interval=.01)
        sampler.start()
        time.sleep(.035)
        data = sampler.finish()
        self.assertGreaterEqual(len(data['samples']), 2)
        times = [r['elapsed_ns'] for r in data['samples']]
        self.assertEqual(times, sorted(set(times)))
        self.assertTrue(all(r['collection_wall_ns'] >= 0 for r in data['samples']))
        self.assertTrue(all(r['rss_bytes'] is None or r['rss_bytes'] > 0 for r in data['samples']))
        self.assertIn('whole diagnostic child', data['scope'])
        self.assertEqual(ctypes.sizeof(RusageV2), 160)
        bad = ProcessReader(2147483647).read()
        self.assertIsNone(bad['rss_bytes'])
        self.assertTrue(bad['errors'])


if __name__ == '__main__':
    unittest.main()
