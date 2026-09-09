"""Read the separate, same-binary generic/fused experiment without speed thresholds."""
import json
from pathlib import Path
from report import checksum


def strategies(data):
    present = {r['strategy'] for r in data['trials']}
    allowed = {'Generic', 'FlatFused', 'FlatFusedCounted'}
    declared = data.get('strategies')
    if declared is not None and (declared not in [['Generic', 'FlatFused'], ['Generic', 'FlatFused', 'FlatFusedCounted']] or set(declared) != present):
        raise ValueError('missing or unknown declared experiment strategies')
    if not present <= allowed:
        raise ValueError('unknown experiment strategy')
    return ('Generic', 'FlatFused', 'FlatFusedCounted') if 'FlatFusedCounted' in present else ('Generic', 'FlatFused')


def read_experiment(directory):
    marker = json.loads((directory / 'complete.json').read_text())
    bound = {name: (size, digest) for name, size, digest in marker['artifacts']}
    if marker['schema_version'] != 1 or 'experiment.json' not in bound:
        raise ValueError('missing experiment completion')
    raw = (directory / 'experiment.json').read_bytes()
    if (len(raw), checksum(raw)) != bound['experiment.json']:
        raise ValueError('modified experiment evidence')
    data = json.loads(raw)
    if data['schema_version'] != 1 or data['kind'] != 'dispatch-experiment':
        raise ValueError('unsupported dispatch experiment')
    if not data['passed']:
        return data
    if data['errors'] or not data['checks'] or any(c['check']['outcome'] != 'pass' for c in data['checks']) or not data['gates'] or any(not g['passed'] for group in data['gates'] for g in group['gates']):
        raise ValueError('experiment claimed passing with failed gates')
    orders = set()
    expected_pairs = {(r, w['id'], s) for r in range(data['replicates']) for w in data['workloads'] for s in strategies(data)}
    groups = {p: [] for p in expected_pairs}
    frames = {w['id']: w['frames'] for w in data['workloads']}
    for record in data['trials']:
        t = record['trial']
        key = (record['replicate'], t['workload'], record['strategy'])
        if key not in groups or t['source'] != 'monotonic' or t['conformance'] != 'pass' or t['candidate'] != 'Offline' or t['frames'] != frames[t['workload']] or t['sample_count'] != t['frames'] or t['block'] != 256:
            raise ValueError('ineligible experiment trial')
        if any(type(t[k]) is not int or t[k] < 0 for k in ('elapsed_ns', 'preparation_ns', 'order', 'trial')) or t['order'] in orders:
            raise ValueError('invalid experiment trial metadata')
        orders.add(t['order'])
        groups[key].append(t)
    for group in groups.values():
        if [t['trial'] for t in group] != list(range(data['repeats_per_replicate'])) or len({t['scope'] for t in group}) != 1:
            raise ValueError('missing, duplicate or mixed-scope experiment repeats')
    for workload in frames:
        if len({r['trial']['scope'] for r in data['trials'] if r['trial']['workload'] == workload}) != 1:
            raise ValueError('mixed-scope strategy comparison')
    return data


def summaries(data):
    if not data['passed']:
        return []
    rows = []
    for w in data['workloads']:
        for replicate in range(data['replicates']):
            row = {'workload': w['id'], 'frames': w['frames'], 'replicate': replicate}
            for strategy in strategies(data):
                group = [r['trial'] for r in data['trials'] if r['replicate'] == replicate and r['strategy'] == strategy and r['trial']['workload'] == w['id']]
                durations = sorted(t['elapsed_ns'] for t in group)
                prep = sorted(t['preparation_ns'] for t in group)
                row[strategy] = dict(median_ns=durations[(len(durations)-1)//2], min_ns=durations[0], max_ns=durations[-1], preparation_median_ns=prep[(len(prep)-1)//2], repeats=len(group))
            row['fused_over_generic'] = row['FlatFused']['median_ns'] / row['Generic']['median_ns'] if row['Generic']['median_ns'] else None
            rows.append(row)
    return rows
