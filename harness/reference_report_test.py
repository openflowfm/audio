"""Synthetic comparison fixtures verify presentation, never measure performance."""
import re
import unittest
from candidates import CLI_CANDIDATES, NAMES
from reference_report import canonical_views
from run import PROFILE_PAIRS, time_chart


def aggregate(candidate, duration):
    return dict(candidate=candidate,workload='fixture',eligible=duration is not None,repeats=0 if duration is None else 2,median_ns=duration,min_ns=duration,max_ns=duration,durations_ns=[] if duration is None else [duration,duration])


class ReferenceReportTests(unittest.TestCase):
    def test_independent_pair_colors_and_denominators_not_a_global_ranking(self):
        rows = [aggregate(c,v) for c,v in [('Offline',100),('Direct',1),('Fundsp',200),('FundspBlock',50),('Oddio',None)]]
        checks = [dict(candidate='Oddio',block=1,outcome='capability_gap',detail='unsupported fixture')]
        document = canonical_views(rows,checks,'test scope','test preparation')
        inventory = document.split('<div class="reference-pair"')[0]
        self.assertNotIn('timing-low',inventory)
        self.assertIn('timing-reference',inventory)
        scalar = document.split('data-reference="Fundsp"')[1].split('<div class="reference-pair"')[0]
        block = document.split('data-reference="FundspBlock"')[1].split('<div class="reference-pair"')[0]
        oddio = document.split('data-reference="Oddio"')[1]
        self.assertIn('class="timing-low ours-row"',scalar)
        self.assertIn('50.00% less',scalar)
        self.assertIn('class="timing-high ours-row"',block)
        self.assertIn('100.00% more',block)
        self.assertIn('FunDSP native block reference denominator',block)
        self.assertIn('No comparison',oddio)
        self.assertIn('Capability gap',oddio)
        self.assertIn('unsupported fixture',oddio)
        self.assertNotIn('timing-low',oddio)
        self.assertNotIn('timing-high',oddio)
        self.assertEqual(document.count('data-reference='),3)

    def test_historical_scalar_evidence_does_not_invent_new_references(self):
        document = canonical_views([aggregate('Offline',100),aggregate('Fundsp',110)],[],'scope','setup')
        self.assertIn('FunDSP scalar reference',document)
        self.assertNotIn('Oddio',document)
        self.assertNotIn('FundspBlock',document)
        self.assertIn('timing-middle',document)

    def test_new_profile_candidates_have_coverage_and_time_series_legend_space(self):
        self.assertEqual(CLI_CANDIDATES['fundsp-block'],'FundspBlock')
        self.assertEqual(CLI_CANDIDATES['oddio'],'Oddio')
        self.assertEqual(len(PROFILE_PAIRS),10)
        chart=time_chart('fixture','units',[(c,[(0,1),(1,2)]) for c in NAMES],'synthetic')
        height=int(re.search(r'viewBox="0 0 740 (\d+)"',chart)[1])
        self.assertGreater(height,343+4*18)
        self.assertIn('Oddio reference',chart)
        self.assertIn('FunDSP native block reference',chart)


if __name__=='__main__':unittest.main()
