#!/usr/bin/env python3
"""Run harness checks and optional measurements, then report this run's evidence."""
import argparse
import datetime
import hashlib
import html
import json
import os
import signal
from pathlib import Path
import platform
import subprocess
import sys
import tempfile
import time

from report import read_study
from candidates import NAMES, SCOPES, CLI_CANDIDATES, RESOURCE_COLORS, present_candidates, LITERAL_KEYS, literals_pass
from reference_report import canonical_views
from dispatch_report import read_experiment, summaries as dispatch_summaries, strategies as dispatch_strategies
from impact_charts import duration_plot, paired_plot, phase_plot, duration_ranks, RANK_STYLE, RANK_NOTE, canonical_comparison, relative_to_fundsp

REPO = Path(__file__).resolve().parent.parent
PROFILE_PAIRS = [(w, c) for w in ('loop-32', 'bursts-16-dense') for c in CLI_CANDIDATES]
CANDIDATES = CLI_CANDIDATES


def save(path, data):
    path.write_text(json.dumps(data, indent=2) + '\n')


def run_stage(directory, name, command, timeout=300):
    """Exit status is authoritative; even launch errors/timeouts retain a stage log."""
    stage = directory / name
    stage.mkdir()
    result = {'name': name, 'command': command, 'status': 'failed', 'exit_code': None,
              'wall_ns': None, 'log': f'{name}/output.txt'}
    start = time.monotonic_ns()
    with (stage / 'output.txt').open('w') as output:
        process = None
        try:
            process = subprocess.Popen(command, cwd=REPO / 'harness', stdout=output,
                                       stderr=subprocess.STDOUT, start_new_session=os.name == 'posix')
            result['exit_code'] = process.wait(timeout=timeout)
            result['status'] = 'passed' if process.returncode == 0 else 'failed'
        except (OSError, subprocess.TimeoutExpired, KeyboardInterrupt) as error:
            if process is not None and process.poll() is None:
                if os.name == 'posix':
                    os.killpg(process.pid, signal.SIGKILL)
                else:
                    process.kill()
                process.wait()
            result['detail'] = type(error).__name__ + ': ' + str(error)
            output.write('\nRunner error: ' + result['detail'] + '\n')
    result['wall_ns'] = time.monotonic_ns() - start
    return result


def only_child(root, prefix):
    children = list(root.glob(prefix + '*'))
    if len(children) != 1:
        raise ValueError(f'expected one {prefix} artifact directory, found {len(children)}')
    return children[0]


def study_evidence(root):
    directory = only_child(root, 'audio-harness-')
    manifest, gates, workloads, trials, aggregates = read_study(directory)
    failed = (any(c['outcome'] == 'fail' for c in gates['checks']) or
              any(not literals_pass(gates,c) for c in present_candidates(aggregates) if c not in ('FundspBlock','Oddio')) or
              bool(gates['execution_errors']) or any(t['conformance'] != 'pass' for t in trials))
    return directory, {'gates': gates, 'aggregates': aggregates, 'trials': len(trials),
                       'workloads': workloads, 'native_gate_requirements': manifest.get('native_gate_requirements', []),
                       'timing_scope': manifest['timing_scope'], 'preparation_scope': manifest['preparation_scope'],
                       'render_strategy': manifest.get('render_strategy'), 'enqueue_strategy': manifest.get('enqueue_strategy')}, failed


def profile_evidence(root):
    directory = only_child(root, 'audio-profile-')
    gates = json.loads((directory / 'gates.json').read_text())
    process = json.loads((directory / 'process.json').read_text())
    selected = {c.get('candidate') for c in gates['checks']}
    native = [c for c in selected if c in ('FundspBlock','Oddio')]
    if native and 'required_native_gate_ids' not in gates:
        raise ValueError('missing native profile gate requirements')
    required_ids = gates.get('required_native_gate_ids', [])
    native_pass = all(literals_pass(gates,c,required_ids) for c in native)
    native_gap = bool(native) and required_ids==[] and bool(gates['checks']) and all(c['outcome']=='capability_gap' for c in gates['checks'])
    other_gates_pass = (bool(gates['literal']) and all(g['passed'] for g in gates['literal']) and
                        all(bool(gates[k]['cases']) and all(c['outcome']=='pass' for c in gates[k]['cases']) for k in ('direct','fundsp')) and
                        (native_pass or native_gap))
    if not gates['passed']:
        gap = (other_gates_pass and
               bool(gates['checks']) and all(c['outcome'] in ('pass', 'capability_gap') for c in gates['checks']) and
               any(c['outcome'] == 'capability_gap' for c in gates['checks']))
        return directory, {'gates': gates, 'process': process}, 'capability_gap' if gap else 'failed'
    if not other_gates_pass or not native_pass or not gates['checks'] or any(c['outcome']!='pass' for c in gates['checks']):
        raise ValueError('profile claimed passing without passing correctness gates')
    marker = json.loads((directory / 'profile-complete.json').read_text())
    required = {'profile.json', 'process.json', 'gates.json', 'artifact-writes.json'}
    if marker['kind'] != 'instrumented-profile' or not required.issubset(marker['sha256']):
        raise ValueError('missing profile completion evidence')
    for name, digest in marker['sha256'].items():
        if Path(name).name != name or hashlib.sha256((directory / name).read_bytes()).hexdigest() != digest:
            raise ValueError('modified profile artifact: ' + name)
    data = json.loads((directory / 'profile.json').read_text())
    if data['kind'] != 'instrumented-profile' or data['comparative_speed_eligible'] or data['completed'] < 1:
        raise ValueError('invalid diagnostic profile')
    data['artifact_write'] = json.loads((directory / 'artifact-writes.json').read_text())
    data['process'] = process
    data['time_series'] = {}
    for key, filename in [('process', 'resource-samples.json'), ('allocator', 'allocation-samples.json')]:
        if filename in marker['sha256']:
            series = json.loads((directory / filename).read_text())
            if series['schema_version'] != 1 or not isinstance(series['samples'], list):
                raise ValueError('invalid time-series schema')
            timestamps = [row['elapsed_ns'] for row in series['samples']]
            if any(type(t) is not int or t < 0 for t in timestamps) or any(b <= a for a, b in zip(timestamps, timestamps[1:])):
                raise ValueError('invalid time-series timestamps')
            data['time_series'][key] = series
    return directory, data, 'passed' if process['exit_code'] == 0 else 'failed'


def table(headers, rows, row_classes=None):
    esc = lambda value: html.escape(str(value))
    row_classes = row_classes or [''] * len(rows)
    return '<div class="scroll"><table><thead><tr>' + ''.join('<th>'+esc(h)+'</th>' for h in headers) + '</tr></thead><tbody>' + ''.join('<tr class="'+esc(css)+'">'+''.join('<td>'+esc(c)+'</td>' for c in row)+'</tr>' for row, css in zip(rows, row_classes)) + '</tbody></table></div>'



def sample_points(samples, field, scale=1, rate=False):
    """Actual sample times and interval deltas; missing/decreasing counters break traces."""
    points = []
    previous = None
    def read(row):
        value = row
        for key in field.split('.'):
            value = value.get(key) if isinstance(value, dict) else None
        return value
    for sample in samples:
        timestamp, value = sample['elapsed_ns'], read(sample)
        observed = None
        if value is not None:
            if not rate:
                observed = value / scale
            elif previous is not None and read(previous) is not None:
                elapsed = timestamp - previous['elapsed_ns']
                delta = value - read(previous)
                if elapsed > 0 and delta >= 0:
                    observed = delta / (elapsed / 1e9) / scale
        points.append((timestamp / 1e9, observed))
        previous = sample
    return points


def time_chart(title, unit, series, note):
    names = NAMES
    colors = RESOURCE_COLORS
    esc = lambda v: html.escape(str(v))
    actual = [(t, v) for _, points in series for t, v in points if v is not None]
    if not actual:
        return '<figure><h4>'+esc(title)+'</h4><p>Unavailable: insufficient valid timestamped observations.</p><figcaption>'+esc(note)+'</figcaption></figure>'
    xmax = max(t for t, _ in actual) or 1
    ymax = max(v for _, v in actual) or 1
    svg = [f'<svg viewBox="0 0 740 {max(390, 352+18*len(series))}" role="img" aria-label="{esc(title)}"><title>{esc(title)} — real elapsed-time samples</title>']
    for step in range(5):
        x = 65 + step / 4 * 605
        y = 275 - step / 4 * 230
        svg.append(f'<path d="M65 {y}H670" stroke="#dce3e9"/><text x="58" y="{y+4}" text-anchor="end">{ymax*step/4:.3g}</text><text x="{x}" y="298" text-anchor="middle">{xmax*step/4:.2f}</text>')
    svg += [f'<text x="65" y="24">{esc(unit)}</text><text x="365" y="322" text-anchor="middle">Elapsed seconds from this sampler’s start</text>']
    for index, (candidate, points) in enumerate(series):
        segments, segment = [], []
        for timestamp, value in points:
            if value is None:
                if segment:
                    segments.append(segment)
                segment = []
                continue
            x, y = 65+timestamp/xmax*605, 275-value/ymax*230
            segment.append(f'{x:.3f},{y:.3f}')
            svg.append(f'<circle cx="{x:.3f}" cy="{y:.3f}" r="2.5" fill="{colors[candidate]}"><title>{esc(names[candidate])}: {timestamp:.6f} s, {value:.6g} {esc(unit)}</title></circle>')
        if segment:
            segments.append(segment)
        for segment in segments:
            svg.append(f'<polyline points="{" ".join(segment)}" fill="none" stroke="{colors[candidate]}" stroke-width="1.5"/>')
        svg.append(f'<text x="65" y="{343+index*18}" fill="{colors[candidate]}">{esc(names[candidate])} · {sum(v is not None for _,v in points)} observations</text>')
    svg.append('</svg>')
    return '<figure><h4>'+esc(title)+'</h4>'+''.join(svg)+'<figcaption>'+esc(note)+'</figcaption></figure>'


def resource_chart(title, unit, groups, note):
    """Grouped bars for actual aggregates; None remains unavailable, never zero."""
    names = NAMES
    colors = RESOURCE_COLORS
    esc = lambda v: html.escape(str(v))
    values = [v for _, rows in groups for _, v in rows if v is not None]
    maximum = max(values, default=0) or 1
    height = 50 + sum(29 + 25 * len(rows) for _, rows in groups)
    svg = [f'<svg viewBox="0 0 740 {height}" role="img" aria-label="{esc(title)}"><title>{esc(title)} — {esc(unit)}</title>']
    y = 22
    for group, rows in groups:
        svg.append(f'<text x="10" y="{y}" font-weight="650">{esc(group)}</text>')
        y += 23
        for candidate, value in rows:
            svg.append(f'<text x="10" y="{y}">{esc(names[candidate])}</text>')
            if value is None:
                svg.append(f'<text x="275" y="{y}">Unavailable</text>')
            else:
                width = value / maximum * 300
                svg.append(f'<rect x="275" y="{y-12}" width="{width:.3f}" height="16" fill="{colors[candidate]}"><title>{esc(names[candidate])}: {value:.6g} {esc(unit)}</title></rect><text x="585" y="{y}">{value:,.4g}</text>')
            y += 25
        y += 6
    svg += [f'<text x="275" y="{height-8}">0 → {maximum:,.4g} {esc(unit)}</text></svg>']
    return '<figure><h4>'+esc(title)+'</h4>'+''.join(svg)+'<figcaption>'+esc(note)+'</figcaption></figure>'


def report(directory):
    """Neutral per-workload data presentation, derived only from retained evidence."""
    data = json.loads((directory / 'run.json').read_text())
    esc = lambda v: html.escape(str(v))
    names = NAMES
    stages = data['stages']
    study = next((s.get('evidence') for s in stages if s['name'] == 'study'), None)
    experiment = next((s.get('evidence') for s in stages if s['name'] == 'dispatch-experiment'), None)
    dispatch_rows = dispatch_summaries(experiment) if experiment else []
    profiles = [s for s in stages if s['name'].startswith('profile-') and 'evidence' in s]
    profile_map = {}
    for stage in profiles:
        e = stage['evidence']
        if 'phases' in e:
            profile_map[(e['workload']['id'], e['candidate'])] = stage
        elif e['gates']['checks']:
            c = e['gates']['checks'][0]
            profile_map[(c['workload'], c['candidate'])] = stage
    inventory = present_candidates(study['aggregates'] if study else [], [c for _,c in profile_map] + ['Offline','Direct','Fundsp'])
    names = {c:NAMES[c] for c in inventory}
    workloads = list(dict.fromkeys(([a['workload'] for a in study['aggregates']] if study else []) + ([w['id'] for w in experiment['workloads']] if experiment else []) + [key[0] for key in profile_map]))
    sections = ['<header><p class="eyebrow">audio[flow] · local measurements</p><h1>Workload statistics</h1><p>Uninstrumented rendering durations and separate diagnostic resource profiles, grouped by workload.</p></header>',
        '<div class="legend">'+''.join('<span class="'+('ours' if c=='Offline' else '')+'">'+esc(names[c])+'<small>'+esc(SCOPES[c])+'</small></span>' for c in names)+'</div>',
        f'<p>Verification run status: <strong>{esc(data["status"])}</strong>. This records correctness/execution status, not comparative performance.</p>',
        '<p class="note">'+RANK_NOTE+'</p>',
        '<nav aria-label="Workloads">'+''.join(f'<a href="#workload-{i}">{esc(w)}</a>' for i, w in enumerate(workloads))+'</nav>']
    if not workloads:
        sections.append('<p>No workload measurements collected. Measurement stages and test output are retained below.</p>')
    for index, workload in enumerate(workloads):
        sections += [f'<section id="workload-{index}"><h2>{esc(workload)}</h2><h3>Canonical rendering durations</h3>']
        selected = [a for a in study['aggregates'] if a['workload'] == workload] if study else []
        if selected:
            sections.append('<p>Canonical audio[flow] selection: '+esc(study.get('render_strategy') or 'not recorded in retained manifest')+' / '+esc(study.get('enqueue_strategy') or 'enqueue selection not recorded')+'.</p>')
            sections += [canonical_views(selected,[c for c in study['gates']['checks'] if c['workload']==workload],study['timing_scope'],study['preparation_scope'],list(names), study.get('gates'), study.get('native_gate_requirements', [])),
                '<details><summary>Timing boundaries and raw repeats</summary>',
                f'<p>Timed: {esc(study["timing_scope"])}. Preparation: {esc(study["preparation_scope"])}.</p>',
                table(['Candidate', 'Retained integer durations (ns)', 'Preparation durations (ns)'], [(names[a['candidate']], ', '.join(map(str, a['durations_ns'])), ', '.join(map(str, a['preparation_ns']))) for a in selected]), '</details>']
        else:
            sections.append('<p>Canonical study not collected for this workload in this run.</p>')
        paired = [r for r in dispatch_rows if r['workload'] == workload]
        if paired:
            sections += ['<h3>Controlled render-dispatch experiment</h3><p>audio[flow] only: Generic control and FlatFused, same optimized binary, alternating trial order. Trial counts and timing scope are shown with the measured distributions. No allocation instrumentation. Generic→FlatFused changes rendering; FlatFused→FlatFusedCounted changes enqueue only when counted trials were collected. Stage names identify measured paths; default selection is recorded by the producing run when available.</p>',
                paired_plot(experiment, workload),
                *( ['<h4>Enqueue experiment — fused renderer held constant</h4>', paired_plot(experiment, workload, ['FlatFused', 'FlatFusedCounted'])] if 'FlatFusedCounted' in dispatch_strategies(experiment) else []),
                '<h4>Rendering comparison statistics — ranked within each replicate</h4>',
                table(['Replicate', 'audio[flow] strategy', 'Median ms', 'Min–max ms', 'Preparation median ms'],
                    [(r['replicate']+1, strategy, f'{r[strategy]["median_ns"]/1e6:.4f}', f'{r[strategy]["min_ns"]/1e6:.4f}–{r[strategy]["max_ns"]/1e6:.4f}', f'{r[strategy]["preparation_median_ns"]/1e6:.4f}') for r in paired for strategy in ('Generic', 'FlatFused')],
                    [rank for r in paired for rank in duration_ranks([r[strategy]['median_ns'] for strategy in ('Generic', 'FlatFused')])])]

        sections += ['<h3>Instrumented resource profiles</h3>']
        measured = [(c, profile_map[(workload, c)]['evidence']) for c in names if (workload, c) in profile_map and 'phases' in profile_map[(workload, c)]['evidence']]
        status_rows = []
        for c in names:
            stage = profile_map.get((workload, c))
            e = stage.get('evidence', {}) if stage else {}
            sampling = e.get('process', {}).get('sampling')
            sample_status = 'Available' if isinstance(sampling, dict) and sampling.get('available') else 'Unavailable / not requested'
            status_rows.append((names[c], stage['status'] if stage else 'Not collected', (e.get('render_strategy', 'Unrecorded') + ' / ' + e.get('enqueue_strategy', 'Unrecorded enqueue')) if c == 'Offline' and 'phases' in e else 'Reference / not collected', e.get('completed', '—'), sample_status if 'phases' in e else 'Not collected'))
        sections.append(table(['Candidate', 'Profile status', 'Offline renderer / enqueue selection', 'Validated trials (normalization denominator)', 'Stack samples'], status_rows).replace('<table>', '<table class="candidate-table">', 1))
        if measured:
            sections.append(phase_plot(measured, names))
            sections += ['<p class="note">CPU and allocation bars divide aggregate totals by that candidate’s validated trial count shown above. CPU includes instrumentation. These bars are phase means, not distributions. Separate measured time-series follow below. Memory high-water marks remain whole-process peaks and are not divided by trial count.</p><div class="charts">']
            phase_names = list(measured[0][1]['phases'])
            sections.append('</div><h4>Work-normalized phase costs</h4><p>CPU ns per output frame divides by validated trials × workload output frames. Allocation requests per workload divide by validated trials. Enqueue per frame is amortized over that workload, not a fixed cost per command. Instructions per frame are unavailable: hardware event identities/per-core deltas have not been validated.</p>')
            for heading, include in [('Engine and adapter work', ('enqueue', 'render')), ('Harness, preparation and validation (not render-kernel attribution)', tuple(n for n in phase_names if n not in ('enqueue', 'render')))]:
                normalized = []
                for candidate, e in measured:
                    frames = e['workload'].get('frames')
                    for phase in include:
                        if phase not in e['phases']:
                            continue
                        p = e['phases'][phase]
                        cpu = p['process_cpu_ns']
                        normalized.append((names[candidate], phase, 'Unavailable' if cpu is None or not frames else f'{cpu/(e["completed"]*frames):.3f}', f'{p["allocations"]/e["completed"]:.3f}'))
                sections += ['<h4>'+esc(heading)+'</h4>', table(['Candidate', 'Phase', 'CPU ns / output frame', 'Allocation requests / complete workload'], normalized)]
            sections.append('<details><summary>Allocation, memory and process resource charts</summary><div class="charts">')
            for title, unit, field, scale in [('Phase CPU per trial', 'ms / trial', 'process_cpu_ns', 1e6), ('Allocation requests per trial', 'requests / trial', 'allocations', 1), ('Requested allocation bytes per trial', 'KiB / trial', 'requested_bytes', 1024)]:
                groups = [(phase.replace('_', ' '), [(c, None if e['phases'][phase][field] is None else e['phases'][phase][field]/e['completed']/scale) for c, e in measured]) for phase in phase_names]
                sections.append(resource_chart(title, unit, groups, 'Rust allocator coverage only for allocation metrics; realloc counts as a free plus a full new request. No profiling counters are added to canonical timing.'))
            memory = [('Whole-child peak RSS', [(c, None if (e['process'].get('process_resources') or {}).get('max_rss_bytes') is None else e['process']['process_resources']['max_rss_bytes']/1048576) for c, e in measured]),
                      ('Maximum Rust live requested bytes across trial phases', [(c, max(p['peak_live_bytes'] for p in e['phases'].values())/1048576) for c, e in measured])]
            sections.append(resource_chart('Memory high-water measurements', 'MiB', memory, 'RSS covers profile-child lifetime including gates/startup. Rust live peaks include still-live harness objects; these are different memory scopes, not exclusive renderer memory.'))
            io = [(field.replace('_', ' '), [(c, None if (e['process'].get('process_resources') or {}).get(field) is None else e['process']['process_resources'][field]/e['completed']) for c, e in measured]) for field in ('input_block_operations', 'output_block_operations', 'minor_faults', 'major_faults')]
            sections.append(resource_chart('Process I/O and faults per validated trial', 'operations / trial', io, 'Whole-child counters divided by completed trials, including startup/gates/writes. Zero may reflect OS accounting/caching. Block operations and page faults are distinct events; neither is disk pressure.'))
            process_cpu = [(field.replace('_', ' '), [(c, None if (e['process'].get('process_resources') or {}).get(field) is None else e['process']['process_resources'][field]*1000/e['completed']) for c, e in measured]) for field in ('user_seconds', 'system_seconds')]
            sections.append(resource_chart('Whole-child CPU per validated trial', 'ms / trial', process_cpu, 'Includes gates, startup and child artifact writes; excludes compiler, launcher and sampler. This denominator amortizes overhead and is not render-only CPU.'))
            sections.append('</div>')
            sections.append('</details>')
            sections += ['<details><summary>Resource usage over elapsed time — diagnostic observations</summary><p>Real timestamped samples from separate diagnostic processes. Curves align by each sampler’s start, not by rendered frames or simultaneous wall-clock execution. Points are observations; lines connect adjacent valid observations and do not reconstruct activity between them. Resource trace colors identify candidates, not performance.</p><div class="charts">']
            plots = [('Cumulative process user CPU', 'ms since process start', 'allocator', 'resources.user_cpu_ns', 1000000, False),
                     ('Cumulative process system CPU', 'ms since process start', 'allocator', 'resources.system_cpu_ns', 1000000, False),
                     ('Cumulative voluntary context switches', 'switches since process start', 'allocator', 'resources.voluntary_context_switches', 1, False),
                     ('Cumulative involuntary context switches', 'switches since process start', 'allocator', 'resources.involuntary_context_switches', 1, False),
                     ('Interval process CPU usage', '% of one CPU', 'allocator', 'process_cpu_ns', 10000000, True),
                     ('Rust allocator request rate', 'requests / s', 'allocator', 'allocations', 1, True),
                     ('Rust requested allocation byte rate', 'MiB / s', 'allocator', 'requested_bytes', 1048576, True),
                     ('Rust live requested bytes at trial boundaries', 'MiB', 'allocator', 'live_requested_bytes', 1048576, False),
                     ('Observed resident memory', 'MiB RSS', 'process', 'rss_bytes', 1048576, False),
                     ('OS-accounted process read rate', 'KiB / s', 'process', 'read_bytes', 1024, True),
                     ('OS-accounted process write rate', 'KiB / s', 'process', 'write_bytes', 1024, True)]
            for title, unit, kind, field, scale, rate in plots:
                series = [(c, sample_points(e.get('time_series', {}).get(kind, {}).get('samples', []), field, scale, rate)) for c, e in measured]
                note = ('Trial-boundary observations approximately every 50 ms after teardown; includes fixture/oracle, preparation and validation. Live memory can miss intra-trial peaks. CPU intervals use actual process CPU deltas divided by elapsed time.' if kind == 'allocator' else 'External OS observations approximately every 50 ms from post-gate handshake through child exit. Includes final metadata/writes; reads have recorded duration. I/O rates are accounting deltas, not device latency, queue depth or disk pressure.')
                sections.append(time_chart(title, unit, series, note))
            sections.append('</div></details>')
        else:
            sections.append('<p>Resource profiling not collected for this workload. No CPU, allocation, memory or I/O values are inferred from canonical durations.</p>')
        for c in names:
            stage = profile_map.get((workload, c))
            if not stage:
                continue
            e = stage['evidence']
            sections.append(f'<details><summary>{esc(names[c])} — raw phase/resource counters and sampling</summary>')
            if 'phases' in e:
                phases = dict(e['phases'], artifact_write_profile_json=e['artifact_write'])
                counters = ['user_cpu_ns', 'system_cpu_ns', 'minor_faults', 'major_faults', 'input_block_operations', 'output_block_operations', 'voluntary_context_switches', 'involuntary_context_switches']
                sections.append('<h4>Phase-scoped process counters (totals)</h4>')
                sections.append(table(['Phase'] + counters, [(n, *[(p.get('resource_deltas') or {}).get(k, 'Unavailable') for k in counters]) for n, p in phases.items()]))
                sections.append('<p>Counter deltas span each instrumented phase; initial process-startup/gate faults are excluded. Probe work is included. Context switches, faults and wall/CPU differences do not identify cache stalls or establish blocking causes.</p>')
                sections.append(table(['Phase', 'Calls', 'Total wall ns', 'Total process CPU ns', 'Allocations', 'Requested bytes', 'Frees', 'Live delta bytes', 'Peak live bytes'], [(n, p['calls'], p['wall_ns'], p['process_cpu_ns'] if p['process_cpu_ns'] is not None else 'Unavailable', p['allocations'], p['requested_bytes'], p['frees'], p['live_delta_bytes'], p['peak_live_bytes']) for n, p in phases.items()]))
            else:
                sections.append('<pre>'+esc(json.dumps(e['gates'], indent=2))+'</pre>')
            sections.append('<pre>'+esc(json.dumps(e['process'], indent=2))+'</pre>')
            sections.append('<details><summary>Raw timestamped observations and sampling scope</summary><pre>'+esc(json.dumps(e.get('time_series', {'status': 'Not collected in this profile'}), indent=2))+'</pre></details>')
            sampling = e['process'].get('sampling')
            if isinstance(sampling, dict) and sampling.get('available'):
                sections.append(f'<p><a href="{esc(stage["artifact_dir"])}/stacks.txt">Real stack samples</a></p>')
            sections.append('</details>')
        if study:
            sections += ['<details><summary>Workload correctness gates</summary>', table(['Candidate', 'Block', 'Outcome', 'Detail'], [(names[c['candidate']], c['block'], c['outcome'], c.get('detail') or '') for c in study['gates']['checks'] if c['workload'] == workload]), '</details>']
        sections.append('</section>')
    sections += ['<section><h2>Measurement scopes</h2><p>Preparation combines asset copy, compilation/adapter setup, state, output and capture buffers. Fixture/oracle work, enqueue, preallocated render, capture, validation and teardown have separate diagnostic counters. Artifact-write counters cover profile.json serialization/write only; gates, sidecar and launcher writes are excluded from that phase.</p>',
        '<p>Rust allocator requests exclude native malloc, fragmentation, mapped memory and allocation backtraces. Stack samples include idle stacks and are not CPU percentages. Per-block probes perturb small operations. Aggregate wait4 disk bytes remain unavailable; timestamped process I/O byte counters are shown when the platform supplies them. Disk latency and queue depth are unavailable; no disk-pressure graph is inferred. Assets reside in RAM. These measurements do not measure native callbacks or streaming.</p></section>',
        '<section><h2>Run evidence</h2><details><summary>Execution stages and actual test output</summary>', table(['Stage', 'Status', 'Exit', 'Wall seconds', 'Detail'], [(s['name'], s['status'], s.get('exit_code', '—'), '—' if s.get('wall_ns') is None else f'{s["wall_ns"]/1e9:.3f}', s.get('detail', '')) for s in stages]),
        '<p>Stage wall durations include builds/startup. Correctness-stage CPU/allocation/RSS/I/O are not instrumented and are unavailable.</p>']
    for stage in stages:
        if 'log' in stage:
            sections.append(f'<details><summary>{esc(stage["name"])} — actual runner output</summary><p><a href="{esc(stage["log"])}">Raw log</a></p><pre>{esc((directory/stage["log"]).read_text(errors="replace"))}</pre></details>')
    sections.append('</details>')
    if study:
        sections.append('<details><summary>All literal checks and execution-error evidence</summary><pre>'+esc(json.dumps(study['gates'], indent=2))+'</pre></details>')
    if experiment:
        sections.append('<details><summary>Raw paired dispatch experiment, gates and build metadata</summary><pre>'+esc(json.dumps(experiment, indent=2))+'</pre></details>')
    sections += ['<details><summary>Environment, exact commands and artifact links</summary>', f'<p>{esc(data["started_utc"])} · {esc(data["platform"])}</p><pre>{esc(json.dumps(data["mode"], indent=2))}</pre>', '<p><a href="run.json">Persisted run data and commands</a> · <a href="completion.json">Evidence hashes</a></p>']
    for stage in stages:
        if 'artifact_dir' in stage:
            sections.append(f'<p>{esc(stage["name"])}: <a href="{esc(stage["artifact_dir"])}/">Retained artifacts</a></p>')
    sections += ['</details></section>']
    style = 'body{font:16px/1.55 system-ui;background:#f5f7fa;color:#17243a;margin:0}main{max-width:1280px;margin:auto;padding:36px 24px}h1{font-size:38px;margin:8px 0}h2{margin:0 0 14px;font-size:28px}h3{margin-top:28px}h4{margin:0 0 12px}.eyebrow{color:#40627b;text-transform:uppercase;font-size:12px;letter-spacing:.12em}section{margin-top:28px;background:white;border:1px solid #dce4e9;border-radius:12px;padding:24px}.legend{display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:12px}.legend>span{padding:18px;background:#edf2f7;border-radius:8px}.legend .ours{font-weight:650}.legend small{display:block;font-size:13px;color:#4f6271}table{border-collapse:collapse;width:100%;font-size:14px}th,td{padding:12px 10px;border-bottom:1px solid #dbe1ea;text-align:left;font-variant-numeric:tabular-nums}th{background:#edf2f7}.scroll{overflow:auto}pre{white-space:pre-wrap;overflow-wrap:anywhere;background:#edf2f7;padding:16px;font-size:12px}details{margin:16px 0;border-top:1px solid #e1e7ed;padding-top:14px}summary{cursor:pointer;font-weight:600}a{color:#1459ad}nav{display:flex;flex-wrap:wrap;gap:12px;margin:24px 0}.charts{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:18px}figure{margin:18px 0;padding:16px;background:#f6f8fa;border-radius:8px;overflow:auto}figure svg{width:100%;min-width:520px;display:block;font:13px system-ui;fill:#17243a}figcaption,.note{font-size:13px;color:#4f6271}@media(max-width:850px){.legend,.charts{grid-template-columns:1fr}main{padding:20px 12px}section{padding:16px}h1{font-size:29px}}'
    style += RANK_STYLE
    document = '<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>audio[flow] — workload statistics</title><style>'+style+'</style><main>'+''.join(sections)+'</main></html>'
    (directory / 'report.html').write_text(document)


def execute(directory, study=False, profiles=False, seconds=2, sample=False, correctness_commands=None, dispatch=False, profile_renderer="flat-fused", profile_enqueue="scan"):
    commands = correctness_commands if correctness_commands is not None else [
        ('format', ['cargo', 'fmt', '--all', '--', '--check']),
        ('build', ['cargo', 'build', '--locked', '--all-targets']),
        ('rust-tests', ['cargo', 'test', '--locked', '--all-targets', '--', '--color', 'never']),
        ('rust-doc-tests', ['cargo', 'test', '--locked', '--doc']),
        ('python-tests', [sys.executable, '-B', '-m', 'unittest', 'discover', '-p', '*_test.py'])]
    data = {'schema_version': 1, 'kind': 'harness-run', 'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'platform': platform.platform(), 'invocation': sys.argv, 'working_directory': str(REPO / 'harness'), 'mode': {'study': study, 'profiles': profiles, 'profile_seconds': seconds, 'sample': sample, 'dispatch_experiment': dispatch, 'profile_renderer': profile_renderer, 'profile_enqueue': profile_enqueue},
            'status': 'running', 'stages': []}
    def retain(stage):
        data['stages'].append(stage)
        save(directory / 'run.json', data)
        save(directory / (stage['name'] + '.json'), stage)
    def skipped(name, reason, status='skipped'):
        retain({'name': name, 'status': status, 'detail': reason})
    try:
        for name, command in commands:
            print(f'Running {name}…', flush=True)
            retain(run_stage(directory, name, command))
        correctness_ok = all(s['status'] == 'passed' for s in data['stages'])
        if study and correctness_ok:
            child_root = directory / 'study-artifacts'
            child_root.mkdir()
            print('Running canonical study…', flush=True)
            stage = run_stage(directory, 'study', ['cargo', 'run', '--locked', '--release', '--bin', 'audio-flow-harness', '--', '--study', '--output-root', str(child_root)])
            try:
                child, evidence, failed = study_evidence(child_root)
                stage.update(artifact_dir=str(child.relative_to(directory)), evidence=evidence)
                if failed:
                    stage.update(status='failed', detail='study correctness or execution failure')
            except Exception as error:
                stage.update(status='failed', detail='study evidence: ' + str(error))
            retain(stage)
        else:
            skipped('study', 'correctness failed' if study else 'not requested; use --full or --study')
        measurement_ok = correctness_ok and not any(s['status'] == 'failed' for s in data['stages'])
        if dispatch and measurement_ok:
            child_root = directory / 'dispatch-artifacts'
            child_root.mkdir()
            print('Running paired generic/fused experiment…', flush=True)
            stage = run_stage(directory, 'dispatch-experiment', ['cargo', 'run', '--locked', '--release', '--bin', 'dispatch-experiment', '--', str(child_root)])
            try:
                child = only_child(child_root, 'audio-harness-')
                evidence = read_experiment(child)
                stage.update(artifact_dir=str(child.relative_to(directory)), evidence=evidence)
                if not evidence['passed']:
                    stage.update(status='failed', detail='dispatch experiment gates or execution failed')
            except Exception as error:
                stage.update(status='failed', detail='dispatch evidence: ' + str(error))
            retain(stage)
        else:
            skipped('dispatch-experiment', 'prior correctness/study failure' if dispatch else 'not requested; use --dispatch-experiment')
        measurement_ok = correctness_ok and not any(s['status'] == 'failed' for s in data['stages'])
        for workload, candidate in PROFILE_PAIRS:
            name = f'profile-{workload}-{candidate}'
            if not profiles or not measurement_ok:
                skipped(name, 'prior correctness/study failure' if profiles else 'not requested; use --full or --profile')
                continue
            child_root = directory / (name + '-artifacts')
            child_root.mkdir()
            print(f'Running {name}…', flush=True)
            command = [sys.executable, str(REPO / 'harness/profile.py'), '--output-root', str(child_root), '--workload', workload, '--candidate', candidate, '--seconds', str(seconds), '--renderer', profile_renderer, '--enqueue', profile_enqueue]
            if sample:
                command.append('--sample')
            stage = run_stage(directory, name, command)
            try:
                child, evidence, status = profile_evidence(child_root)
                stage.update(artifact_dir=str(child.relative_to(directory)), evidence=evidence)
                # A nonzero process exit is allowed only for a proven capability gap.
                if (status == 'capability_gap' and stage['exit_code'] == 1 and evidence['process']['exit_code'] == 1) or stage['status'] == 'passed':
                    stage['status'] = status
            except Exception as error:
                stage.update(status='failed', detail='profile evidence: ' + str(error))
            retain(stage)
    except Exception as error:
        retain({'name': 'runner-error', 'status': 'failed', 'detail': str(error)})
    data['status'] = 'failed' if any(s['status'] == 'failed' for s in data['stages']) else 'passed'
    save(directory / 'run.json', data)
    report(directory)
    hashes = {str(p.relative_to(directory)): hashlib.sha256(p.read_bytes()).hexdigest() for p in directory.rglob('*') if p.is_file()}
    save(directory / 'completion.json', {'kind': 'harness-run', 'status': data['status'], 'sha256': hashes})
    return 1 if data['status'] == 'failed' else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-root', type=Path, default=Path(tempfile.gettempdir()))
    parser.add_argument('--full', action='store_true', help='checks, canonical study, bounded profile matrix and report')
    parser.add_argument('--study', action='store_true', help='also run canonical offline throughput')
    parser.add_argument('--profile', action='store_true', help='also run the diagnostic profile matrix')
    parser.add_argument('--profile-seconds', type=int, default=2, help='1..60 seconds per eligible profile (default 2)')
    parser.add_argument('--dispatch-experiment', action='store_true', help='same-binary paired Generic/FlatFused experiment after checks')
    parser.add_argument('--profile-renderer', choices=['generic', 'flat-fused'], default='flat-fused', help='diagnostic Offline profile path only; canonical renderer is selected by the study binary')
    parser.add_argument('--profile-enqueue', choices=['scan', 'counted'], default='scan', help='diagnostic Offline enqueue selection; canonical study remains scan')
    parser.add_argument('--sample', action='store_true', help='request macOS stack sampling during profiles')
    args = parser.parse_args()
    root = args.output_root.resolve(strict=True)
    if not root.is_dir() or root == REPO or REPO in root.parents:
        parser.error('output root must be an existing directory outside the repository')
    if not 1 <= args.profile_seconds <= 60:
        parser.error('profile seconds must be 1..60')
    directory = Path(tempfile.mkdtemp(prefix='audio-run-', dir=root))
    print(f'Run directory: {directory}', flush=True)
    code = execute(directory, args.full or args.study, args.full or args.profile, args.profile_seconds, args.sample, dispatch=args.dispatch_experiment, profile_renderer=args.profile_renderer, profile_enqueue=args.profile_enqueue)
    print(f'Report: {directory / "report.html"}', flush=True)
    return code


if __name__ == '__main__':
    raise SystemExit(main())
