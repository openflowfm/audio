"""Runner behavior tests use real subprocess exits, never synthetic speed evidence."""
import contextlib
import hashlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest

import run


class RunnerTests(unittest.TestCase):
    def execute(self, root, commands, **options):
        with contextlib.redirect_stdout(io.StringIO()):
            code = run.execute(root, correctness_commands=commands, **options)
        return code, json.loads((root / 'run.json').read_text())

    def test_success_reports_actual_output_and_skipped_measurements(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            code, data = self.execute(root, [('real-check', [sys.executable, '-c', 'print("<actual output>")'])])
            self.assertEqual(code, 0)
            self.assertEqual(data['stages'][0]['exit_code'], 0)
            self.assertGreater(data['stages'][0]['wall_ns'], 0)
            self.assertTrue(all(s['status'] == 'skipped' for s in data['stages'][1:]))
            document = (root / 'report.html').read_text()
            self.assertIn('&lt;actual output&gt;', document)
            self.assertIn('not requested', document)
            hashes = json.loads((root / 'completion.json').read_text())['sha256']
            self.assertTrue(all(hashlib.sha256((root / p).read_bytes()).hexdigest() == digest for p, digest in hashes.items()))
            run.report(root)  # Regeneration consumes stored results only.
            self.assertEqual(document, (root / 'report.html').read_text())

    def test_real_test_failure_propagates_and_retains_report_before_measurements(self):
        program = ('import unittest,sys\n'
                   'class Broken(unittest.TestCase):\n'
                   ' def test_failure(self): self.fail("intentional runner propagation probe")\n'
                   'r=unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(Broken))\n'
                   'sys.exit(not r.wasSuccessful())')
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            code, data = self.execute(root, [('failing-test', [sys.executable, '-c', program]),
                                            ('independent-check', [sys.executable, '-c', 'print("continued")'])], study=True, profiles=True)
            self.assertEqual(code, 1)
            self.assertEqual(data['status'], 'failed')
            self.assertEqual(data['stages'][0]['exit_code'], 1)
            self.assertEqual(data['stages'][1]['status'], 'passed')
            self.assertTrue(all(s['status'] == 'skipped' for s in data['stages'][2:]))
            document = (root / 'report.html').read_text()
            self.assertIn('intentional runner propagation probe', document)
            self.assertIn('correctness failed', document)
            self.assertTrue((root / 'completion.json').exists())
            self.assertFalse((root / 'study-artifacts').exists())

    def test_launch_error_and_timeout_are_failed_evidence(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            missing = run.run_stage(root, 'missing', ['/no/such/audio-harness-command'])
            timed = run.run_stage(root, 'timeout', [sys.executable, '-c', 'import time; time.sleep(5)'], timeout=.02)
            for stage in (missing, timed):
                self.assertEqual(stage['status'], 'failed')
                self.assertIn('Runner error', (root / stage['log']).read_text())

    def test_gap_requires_all_other_gates_pass_and_profile_hashes_are_checked(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            child = root / 'audio-profile-fixture'
            child.mkdir()
            gates = {'passed': False, 'literal': [{'passed': True}],
                     'direct': {'cases': [{'outcome': 'pass'}]}, 'fundsp': {'cases': [{'outcome': 'pass'}]},
                     'checks': [{'outcome': 'capability_gap'}]}
            run.save(child / 'gates.json', gates)
            run.save(child / 'process.json', {'exit_code': 1})
            self.assertEqual(run.profile_evidence(root)[2], 'capability_gap')
            gates['literal'][0]['passed'] = False
            run.save(child / 'gates.json', gates)
            self.assertEqual(run.profile_evidence(root)[2], 'failed')
            gates['passed'] = True
            run.save(child / 'gates.json', gates)
            run.save(child / 'profile-complete.json', {'kind': 'instrumented-profile', 'sha256': {}})
            with self.assertRaisesRegex(ValueError, 'completion'):
                run.profile_evidence(root)


if __name__ == '__main__':
    unittest.main()
