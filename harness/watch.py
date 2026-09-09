#!/usr/bin/env python3
"""Loopback live report server; invokes verify without changing its evidence."""
import argparse
import fcntl
import hashlib
import html
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import mimetypes
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import threading
import time
from urllib.parse import quote, unquote, urlsplit

REPO = Path(__file__).resolve().parent.parent
EXCLUDED = {'.git', 'target', '__pycache__', 'node_modules', '.venv', 'venv', '.DS_Store'}
PAGE = '''<!doctype html><html lang="en"><meta charset="utf-8"><title>audio[flow] live reports</title>
<style>body{margin:0;font:14px system-ui;color:#17243a;background:#f5f7fa}header{padding:12px 20px}#history{max-height:100px;overflow:auto}iframe{width:100%;height:calc(100vh - 180px);border:0;background:white}a{color:#1459ad}</style>
<header><b>audio[flow] live reports</b><p id="status">Connecting…</p><div id="history"></div></header><iframe title="Latest completed report" id="report"></iframe>
<script>
let shown=null;
async function refresh(){try{
 const s=await (await fetch('/status',{cache:'no-store'})).json();
 document.getElementById('status').textContent = s.running ? 'Running '+s.running+' · '+(s.stale?'inputs changed; one rerun queued':'current input snapshot')+' · previous report remains below' : (s.stale?'Inputs changed · waiting for edits to settle': 'Watching · '+(s.latest?s.latest.status:'no completed run'));
 const h=document.getElementById('history');h.replaceChildren();
 for(const r of s.history.slice().reverse()) {const a=document.createElement('a');a.textContent=r.id+' · '+r.status+(r.stale?' · stale inputs':'');a.href=r.url;a.target='_blank';h.append(a,document.createElement('br'));}
 if(s.latest && s.latest.url!==shown){shown=s.latest.url;document.getElementById('report').src=shown;}
 }catch(e){document.getElementById('status').textContent='Watcher disconnected · showing last report';}finally{setTimeout(refresh,1000);}}
refresh();</script></html>'''


def snapshot(repo):
    """Hash all checkout inputs, including new/deleted files, ignoring build evidence."""
    result = {}
    for base, dirs, files in os.walk(repo, followlinks=False):
        dirs[:] = sorted(d for d in dirs if d not in EXCLUDED and not d.startswith(('audio-run-', 'audio-profile-', 'audio-harness-')))
        for name in sorted(files):
            if name in EXCLUDED or name.endswith(('.pyc', '.swp', '.swo', '~')):
                continue
            path = Path(base) / name
            try:
                if path.is_symlink():
                    data = os.readlink(path).encode() + path.read_bytes()
                else:
                    data = path.read_bytes()
                result[str(path.relative_to(repo))] = hashlib.sha256(data).hexdigest()
            except OSError:
                # A rename/atomic save racing a scan becomes another change next scan.
                result[str(path.relative_to(repo))] = 'unreadable'
    return result


def digest(inputs):
    return hashlib.sha256(json.dumps(inputs, sort_keys=True).encode()).hexdigest()


def save(path, data):
    temp = path.with_suffix(path.suffix + '.tmp')
    temp.write_text(json.dumps(data, indent=2) + '\n')
    temp.replace(path)


def cleanup(process):
    """Runner stages start new sessions, so terminate the descendant tree as well."""
    if process.poll() is not None:
        return
    descendants = {process.pid}
    stopped = set()
    try:
        # Freeze each generation before scanning again; children cannot escape by
        # starting a new session (as runner stages and profilers legitimately do).
        while descendants != stopped:
            for pid in descendants - stopped:
                try:
                    os.kill(pid, signal.SIGSTOP)
                except ProcessLookupError:
                    pass
            stopped = descendants.copy()
            rows = subprocess.check_output(['ps', '-axo', 'pid=,ppid='], text=True)
            pairs = [tuple(map(int, row.split())) for row in rows.splitlines()]
            descendants |= {pid for pid, parent in pairs if parent in descendants}
    finally:
        for pid in sorted(descendants, reverse=True):
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
    process.wait()


class Watcher:
    def __init__(self, repo, root, debounce=1.5, command=None):
        self.repo, self.root, self.debounce = repo.resolve(), root.resolve(), debounce
        repo, root = self.repo, self.root
        self.command = command or [str(repo / 'harness/verify'), '--full', '--profile-seconds', '2', '--sample']
        self.inputs = snapshot(repo)
        self.changed_at = time.monotonic() - debounce
        self.pending = True
        self.process = None
        self.history = []
        for path in sorted(root.glob('attempt-*/watch.json')):
            try:
                item = json.loads(path.read_text())
                if item.get('status') != 'running':
                    self.history.append(item)
            except (OSError, ValueError):
                pass
        self.history.sort(key=lambda item: item.get('started', 0))
        self.state = {}
        self.publish()

    def publish(self):
        self.state = dict(running=self.attempt.name if self.process else None,
                          stale=self.pending, history=self.history,
                          latest=self.history[-1] if self.history else None)

    def tick(self):
        current = snapshot(self.repo)
        if current != self.inputs:
            self.inputs, self.changed_at, self.pending = current, time.monotonic(), True
        if self.process and self.process.poll() is not None:
            self.finish()
        if not self.process and self.pending and time.monotonic() - self.changed_at >= self.debounce:
            self.start()
        self.publish()

    def start(self):
        self.attempt = Path(tempfile.mkdtemp(prefix='attempt-' + time.strftime('%Y%m%d-%H%M%S-'), dir=self.root))
        self.started_inputs = self.inputs.copy()
        self.pending = False
        save(self.attempt / 'inputs.json', self.started_inputs)
        self.item = dict(id=self.attempt.name, status='running', started=time.time(), input_sha256=digest(self.started_inputs))
        save(self.attempt / 'watch.json', self.item)
        self.log = (self.attempt / 'watch.log').open('w')
        try:
            self.process = subprocess.Popen(self.command + ['--output-root', str(self.attempt)], cwd=self.repo,
                                            stdout=self.log, stderr=subprocess.STDOUT, start_new_session=True)
        except OSError as error:
            self.log.write(str(error))
            self.finish(error=str(error))

    def finish(self, error=None):
        code = self.process.returncode if self.process else None
        self.log.close()
        reports = sorted(self.attempt.glob('audio-run-*/report.html'))
        status = 'passed' if code == 0 and reports and not error else 'failed'
        if error:
            status = 'interrupted' if error == 'watcher stopped' else 'failed'
        if reports:
            report = reports[-1]
        else:
            report = self.attempt / 'report.html'
            report.write_text('<!doctype html><meta charset="utf-8"><h1>Run '+status+'</h1><p>'+html.escape(error or 'Runner exited without a report')+'</p><a href="watch.log">Runner log</a>')
        self.item.update(status=status, exit_code=code, finished=time.time(), stale=self.pending,
                         url='/artifacts/' + quote(str(report.relative_to(self.root)), safe='/'))
        save(self.attempt / 'watch.json', self.item)
        self.history = self.history + [self.item.copy()]
        self.process = None

    def close(self):
        if self.process:
            cleanup(self.process)
            self.finish(error='watcher stopped')
        self.publish()


def handler(watcher):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            route = unquote(urlsplit(self.path).path)
            if route == '/':
                return self.respond(PAGE.encode(), 'text/html; charset=utf-8')
            if route == '/status':
                return self.respond(json.dumps(watcher.state).encode(), 'application/json')
            if not route.startswith('/artifacts/'):
                return self.send_error(404)
            relative = route[len('/artifacts/'):]
            parts = Path(relative).parts
            if not parts or not parts[0].startswith('attempt-') or '..' in parts:
                return self.send_error(404)
            path = (watcher.root / relative).resolve()
            allowed = (watcher.root / parts[0]).resolve()
            if allowed.parent != watcher.root or path != allowed and allowed not in path.parents:
                return self.send_error(404)
            try:
                if path.is_dir():
                    links = []
                    for child in sorted(path.iterdir()):
                        resolved = child.resolve()
                        if allowed not in resolved.parents:
                            continue
                        url = '/artifacts/' + quote(str(child.relative_to(watcher.root)), safe='/')
                        links.append('<li><a href="'+url+'">'+html.escape(child.name)+'</a></li>')
                    return self.respond(('<!doctype html><meta charset="utf-8"><ul>'+''.join(links)+'</ul>').encode(), 'text/html; charset=utf-8')
                return self.respond(path.read_bytes(), mimetypes.guess_type(path.name)[0] or 'application/octet-stream')
            except (OSError, ValueError):
                return self.send_error(404)

        def respond(self, data, mime):
            self.send_response(200)
            self.send_header('Content-Type', mime)
            self.send_header('Cache-Control', 'no-store')
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            try:
                self.wfile.write(data)
            except (BrokenPipeError, ConnectionResetError):
                pass
    return Handler


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-root', type=Path, required=True, help='dedicated existing directory outside checkout')
    parser.add_argument('--port', type=int, default=8765)
    parser.add_argument('--debounce', type=float, default=1.5)
    parser.add_argument('--dispatch-experiment', action='store_true',
                        help='forward paired Generic/FlatFused experiment to verify')
    args = parser.parse_args()
    root = args.output_root.resolve(strict=True)
    if not root.is_dir() or root == REPO or REPO in root.parents or args.debounce < 0:
        parser.error('use an external output directory and nonnegative debounce')
    # One watcher per checkout, even when callers choose different output roots/ports.
    lockpath = Path(tempfile.gettempdir()) / ('audio-watch-' + hashlib.sha256(str(REPO).encode()).hexdigest()[:16] + '.lock')
    lock = lockpath.open('a+')
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        parser.error('a watcher already owns this checkout')
    watcher = Watcher(REPO, root, args.debounce)
    if args.dispatch_experiment:
        watcher.command.append('--dispatch-experiment')
    server = ThreadingHTTPServer(('127.0.0.1', args.port), handler(watcher))
    stop = threading.Event()
    for sig in (signal.SIGTERM, signal.SIGINT):
        signal.signal(sig, lambda *_: stop.set())
    url = 'http://127.0.0.1:' + str(server.server_port) + '/'
    save(root / 'server.json', dict(pid=os.getpid(), url=url, repo=str(REPO)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    print(url, flush=True)
    try:
        while not stop.is_set():
            watcher.tick()
            stop.wait(0.5)
    finally:
        watcher.close()
        server.shutdown()
        server.server_close()
        save(root / 'server.json', dict(pid=None, url=url, repo=str(REPO), status='stopped'))
        lock.close()


if __name__ == '__main__':
    main()
