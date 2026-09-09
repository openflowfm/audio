"""Live orchestration tests use tiny subprocesses, never nested benchmark runs."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from urllib.error import HTTPError
from urllib.request import urlopen
from http.server import ThreadingHTTPServer

from watch import Watcher, handler, snapshot

FAKE = '''import pathlib, sys, time
root=pathlib.Path(sys.argv[-1]); child=root/'audio-run-fixture'; child.mkdir()
time.sleep(.15)
(child/'report.html').write_text('completed')
sys.exit(1 if (pathlib.Path.cwd()/'fail').exists() else 0)
'''


class WatchTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.base = Path(self.temp.name)
        self.repo = self.base / 'repo'
        self.root = self.base / 'evidence'
        self.repo.mkdir()
        self.root.mkdir()
        self.watcher = Watcher(self.repo, self.root, debounce=.04, command=[sys.executable, '-c', FAKE])

    def tearDown(self):
        self.watcher.close()
        self.temp.cleanup()

    def until(self, predicate):
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            self.watcher.tick()
            if predicate():
                return
            time.sleep(.01)
        self.fail('watcher did not reach expected state')

    def test_inputs_content_create_delete_and_exclusions(self):
        for directory in ('src', 'tests', 'fixtures', '.cargo'):
            (self.repo / directory).mkdir()
            (self.repo / directory / 'input').write_text('a')
        before = snapshot(self.repo)
        self.assertIn('.cargo/input', before)
        for directory in ('target', '__pycache__', '.git', 'audio-run-output'):
            (self.repo / directory).mkdir()
            (self.repo / directory / 'ignored').write_text('x')
        self.assertEqual(snapshot(self.repo), before)
        (self.repo / 'src/input').write_text('b')
        self.assertNotEqual(snapshot(self.repo), before)
        (self.repo / 'Cargo.lock').write_text('dependency')
        self.assertIn('Cargo.lock', snapshot(self.repo))
        (self.repo / 'fixtures/input').unlink()
        self.assertNotIn('fixtures/input', snapshot(self.repo))

    def test_debounce_serial_queued_rerun_and_failed_history(self):
        w = self.watcher
        self.until(lambda: w.process is not None)
        pid = w.process.pid
        for index in range(4):
            (self.repo / 'fail').write_text(str(index))
            w.tick()
            self.assertEqual(w.process.pid, pid)
            self.assertTrue(w.state['stale'])
        self.until(lambda: len(w.history) == 1 and w.process is not None)
        self.assertTrue(w.history[0]['stale'])
        self.assertEqual(w.state['latest'], w.history[0])
        self.until(lambda: len(w.history) == 2 and w.process is None)
        self.assertEqual([r['status'] for r in w.history], ['failed', 'failed'])
        time.sleep(.06)
        w.tick()
        self.assertEqual(len(w.history), 2)
        (self.repo / 'fail').unlink()
        w.tick()
        self.assertIsNone(w.process)  # debounce holds the new run
        self.until(lambda: len(w.history) == 3)
        self.assertEqual(w.history[-1]['status'], 'passed')
        self.assertEqual(len(list(self.root.glob('attempt-*/inputs.json'))), 3)

    def test_http_status_refresh_history_and_confinement(self):
        self.until(lambda: len(self.watcher.history) == 1)
        server = ThreadingHTTPServer(('127.0.0.1', 0), handler(self.watcher))
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        base = 'http://127.0.0.1:' + str(server.server_port)
        try:
            with urlopen(base) as response:
                page = response.read().decode()
                self.assertIn("fetch('/status'", page)
                self.assertIn(".src=shown", page)
                self.assertEqual(response.headers['Cache-Control'], 'no-store')
            first = json.load(urlopen(base + '/status'))['latest']['url']
            self.assertEqual(urlopen(base + first).read(), b'completed')
            (self.repo / 'new-test').write_text('x')
            self.until(lambda: len(self.watcher.history) == 2)
            second = json.load(urlopen(base + '/status'))['latest']['url']
            self.assertNotEqual(first, second)
            self.assertEqual(urlopen(base + first).read(), b'completed')
            attempt = self.root / self.watcher.history[-1]['id']
            (attempt / 'escape').symlink_to(self.repo)
            for path in ('/status/../../watch.py', '/artifacts/../repo/new-test',
                         '/artifacts/' + attempt.name + '/escape/new-test',
                         '/artifacts/' + attempt.name + '/%2e%2e/server.json'):
                with self.assertRaises(HTTPError) as result:
                    urlopen(base + path)
                self.assertEqual(result.exception.code, 404)
                result.exception.close()
        finally:
            server.shutdown()
            server.server_close()
            thread.join()

    def test_restart_recovers_history_and_separate_session_child_cleanup(self):
        self.until(lambda: len(self.watcher.history) == 1)
        recovered = Watcher(self.repo, self.root, command=[sys.executable, '-c', FAKE])
        self.assertEqual(recovered.history, self.watcher.history)
        self.assertTrue(recovered.pending)
        pidfile = self.repo / 'child-pid'
        child_code = 'import time; time.sleep(30)'
        parent_code = ("import pathlib, subprocess, sys, time; "
                       "p=subprocess.Popen([sys.executable, '-c', " + repr(child_code) + "], start_new_session=True); "
                       "pathlib.Path(" + repr(str(pidfile)) + ").write_text(str(p.pid)); time.sleep(30)")
        self.watcher.command = [sys.executable, '-c', parent_code]
        self.watcher.pending = True
        self.watcher.tick()
        deadline = time.monotonic() + 3
        while not pidfile.exists() and time.monotonic() < deadline:
            time.sleep(.01)
        self.assertTrue(pidfile.exists())
        pid = int(pidfile.read_text())
        self.watcher.close()
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            result = subprocess.run(['ps', '-p', str(pid), '-o', 'stat='], capture_output=True, text=True)
            if result.returncode != 0 or result.stdout.strip().startswith('Z'):
                break
            time.sleep(.01)
        else:
            self.fail('separate-session descendant survived shutdown')

    def test_launch_failure_and_stop_retain_report(self):
        self.watcher.command = ['/missing/audio-watcher-test-executable']
        self.watcher.tick()
        self.assertEqual(self.watcher.history[-1]['status'], 'failed')
        self.assertTrue((self.root / self.watcher.history[-1]['id'] / 'report.html').exists())
        self.watcher.command = [sys.executable, '-c', 'import time; time.sleep(30)']
        self.watcher.pending = True
        self.watcher.tick()
        process = self.watcher.process
        self.watcher.close()
        self.assertIsNotNone(process.poll())
        self.assertEqual(self.watcher.history[-1]['status'], 'interrupted')


if __name__ == '__main__':
    unittest.main()
