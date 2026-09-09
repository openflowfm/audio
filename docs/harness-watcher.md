# Live report watcher

Start from the saved checkout on macOS or Linux (Python standard library only):

```sh
mkdir -p /tmp/audio-flow-live
./harness/watch --output-root /tmp/audio-flow-live --port 8765
```

Leave <http://127.0.0.1:8765/> open. The page polls status every second and loads each newly completed report automatically. During a run the previous report stays visible. History links retain both successful and failed reports, including their linked raw evidence. The wrapper does not edit generated runner reports or hashes.

The initial run and every subsequent run invoke `./harness/verify --full --profile-seconds 2 --sample`. This runs correctness checks, then the canonical study and diagnostic profile matrix when prerequisite checks pass. The runner controls skip/failure semantics. No benchmark logic is duplicated. Add `--dispatch-experiment` to the watcher command to include the runner’s paired Generic/FlatFused experiment in every run; this keeps Generic as the default canonical renderer. Avoid launching other measurement commands concurrently: the watcher serializes its own runs and prevents a second watcher on the same checkout, but cannot serialize unrelated manual benchmark processes.

Every half second the watcher hashes checkout inputs, including implementation, tests, fixtures, Cargo manifests/lockfiles, build scripts, Python/report code, configuration, documentation, and newly created/deleted files. Unchanged content and timestamp-only touches do not rerun checks. `.git`, `target`, Python caches, virtual environments, `node_modules`, editor swap/backup files, and generated `audio-run-*`, `audio-profile-*`, `audio-harness-*` directories are excluded. Keep generated output outside the checkout; the runner and watcher enforce an external output root. Edits settle for 1.5 seconds (`--debounce` changes this). Edits during a run mark it stale and coalesce into one subsequent run after edits settle; the in-flight measurement is allowed to finish. Input hashes are retained in each attempt so results from a changing checkout remain identifiable; runs use the live checkout, not an immutable source snapshot.

Use a dedicated output directory. Each `attempt-*` contains `inputs.json`, `watch.json`, `watch.log` and the runner's unique `audio-run-*` tree. Launch failures and interruptions also retain a small report and log. History is recovered on restart, and an initial full run always refreshes it. No historical evidence is deleted. `/tmp` is convenient for a local session but OS cleanup may remove it; use a durable external directory for long-term evidence.

Stop a foreground watcher with Ctrl-C. For a background instance, read its PID from `OUTPUT_ROOT/server.json` and send `kill -TERM PID`. Shutdown kills the runner and descendant stages (which may use separate process sessions), retains interruption status, closes the HTTP server, and releases the checkout lock. `server.json` then records a stopped state. Abrupt SIGKILL or machine termination cannot perform cleanup; prefer SIGTERM. Start with the same command/output root to recover history. The server binds only to 127.0.0.1 and exposes only attempt artifacts, with traversal and symlink-escape checks. It is intended for trusted local use.

Validation:

```sh
python3 -B -m unittest discover -s harness -p watch_test.py
```

These tests use short fixture subprocesses and an ephemeral loopback port, exercising content changes, debounce/queued reruns, failure history, process cleanup, status polling, new report URLs, and filesystem confinement. They require permission to bind loopback sockets and inspect child processes.
