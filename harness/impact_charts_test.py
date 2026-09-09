"""Synthetic chart-contract fixtures, never performance evidence."""
import unittest
from impact_charts import duration_plot, paired_plot, phase_plot, canonical_comparison, relative_to_fundsp, duration_unit


class ImpactChartTests(unittest.TestCase):
    def test_zero_gap_and_shared_axis_preserve_observations(self):
        chart = duration_plot('test', [('ours', [0, 1000000, 2000000]), ('reference', [])], 'enqueue + render')
        self.assertEqual(chart.count('<circle'), 3)
        self.assertIn('1.0000 ms median', chart)
        self.assertIn('Unavailable / capability gap', chart)
        self.assertIn('cx="300.000"', chart)
        self.assertIn('n = 0', chart)
        self.assertIn('enqueue + render', chart)

    def test_delta_is_difference_of_replica_medians_and_signed(self):
        data = {'replicates': 1, 'trials': [dict(replicate=0, strategy=s, trial=dict(workload='w', elapsed_ns=n, scope='scope')) for s,n in [('Generic',1000000),('Generic',3000000),('FlatFused',2000000),('FlatFused',4000000)]]}
        chart = paired_plot(data, 'w')
        self.assertIn('-1.0000 ms saved (-100.00%)', chart)
        self.assertEqual(chart.count('<circle'), 4)
        self.assertNotIn('ours-better', chart)
        data['trials'][0]['trial']['elapsed_ns'] = 0
        self.assertIn('percentage unavailable (zero control)', paired_plot(data, 'w'))

    def test_enqueue_comparison_does_not_include_generic(self):
        data = {'replicates': 1, 'trials': [dict(replicate=0, strategy=s, trial=dict(workload='w', elapsed_ns=n, scope='scope')) for s,n in [('Generic',9000000),('FlatFused',2000000),('FlatFusedCounted',1000000)]]}
        chart = paired_plot(data, 'w', ['FlatFused','FlatFusedCounted'])
        self.assertNotIn('Generic', chart)
        self.assertIn('1.0000 ms saved (50.00%)', chart)
        self.assertEqual(chart.count('<circle'),2)

    def test_chart_colors_follow_medians_and_pair_groups(self):
        rows = [('ours', [200]), ('direct', [100]), ('reference', [300]), ('gap', [])]
        chart = duration_plot('test', rows, 'scope', bold_indices=(0,))
        self.assertIn('data-rank="timing-middle" font-weight="700"', chart)
        self.assertIn('data-rank="timing-low" font-weight="400"', chart)
        self.assertIn('data-rank="timing-high" font-weight="400"', chart)
        self.assertIn('data-rank="" font-weight="400"', chart)
        chart = duration_plot('pairs', [('a',[100]),('b',[90]),('a',[5]),('b',[6])], 'scope', comparison_groups=[0,0,1,1])
        import re
        self.assertEqual(re.findall('data-rank="([^"]*)"',chart), ['timing-high','timing-low','timing-low','timing-high'])

    def test_direct_reference_is_excluded_and_threshold_is_inclusive(self):
        candidates = ['Offline','Direct','Fundsp']
        self.assertEqual(canonical_comparison(candidates,[110,1,100])[0], ['timing-middle','timing-reference','timing-low'])
        self.assertEqual(canonical_comparison(candidates,[111,999,100])[0], ['timing-high','timing-reference','timing-low'])
        ranks, notes = canonical_comparison(candidates,[100,0,110])
        self.assertEqual(ranks,['timing-low','timing-reference','timing-middle'])
        self.assertEqual(notes,['0.00% vs faster','Reference only','10.00% slower'])
        self.assertEqual(canonical_comparison(candidates,[100,1,100])[1], ['Tied (0.00%)','Reference only','Tied (0.00%)'])
        self.assertEqual(canonical_comparison(candidates,[100,None,None])[0], ['', 'timing-reference',''])
        self.assertEqual(canonical_comparison(candidates,[None,100,200])[0], ['', 'timing-reference',''])
        self.assertEqual(canonical_comparison(candidates,[0,1,2])[1][-1], 'vs zero: undefined %')
        chart = duration_plot('canonical', [('ours',[110]),('direct',[1]),('fundsp',[100])], 'scope', canonical_candidates=candidates, bold_indices=(0,))
        self.assertIn('10.00% slower', chart)
        self.assertIn('data-rank="timing-reference"',chart)
        self.assertIn('fill="#2875bf"',chart)

    def test_relative_time_direction_ratio_missing_and_auto_units(self):
        self.assertEqual(relative_to_fundsp(800000,1000000), ('200 µs less','20.00% less','0.8×'))
        self.assertEqual(relative_to_fundsp(1200000,1000000), ('200 µs more','20.00% more','1.2×'))
        self.assertEqual(relative_to_fundsp(10,10), ('Tied (0 ns)','Tied (0.00%)','1×'))
        self.assertEqual(relative_to_fundsp(None,10), ('Unavailable',)*3)
        self.assertEqual(relative_to_fundsp(10,None), ('Unavailable',)*3)
        self.assertEqual(relative_to_fundsp(1,0), ('1 ns more','Undefined (FunDSP = 0)','Undefined (FunDSP = 0)'))
        self.assertEqual([duration_unit(v) for v in (1,1500,2500000,3200000000)], ['1 ns','1.5 µs','2.5 ms','3.2 s'])

    def test_phase_denominator_missing_phase_and_no_wall_substitution(self):
        e = dict(completed=2, workload={'frames':100}, phases={'enqueue':{'process_cpu_ns':200}, 'render':{'process_cpu_ns':600, 'wall_ns':999999999}})
        chart = phase_plot([('Offline',e)], {'Offline':'ours'})
        self.assertIn('enqueue: 1.000', chart)
        self.assertIn('render: 3.000', chart)
        self.assertIn('capture: unavailable', chart)
        self.assertIn('Partial / unavailable', chart)
        self.assertNotIn('999999999', chart)


if __name__ == '__main__':
    unittest.main()
