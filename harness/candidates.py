"""Stable evidence IDs and explicit adapter identities for retained reports."""
NAMES = {
    'Offline': 'audio[flow] MVP (ours)',
    'Direct': 'Direct reference',
    'Fundsp': 'FunDSP scalar reference',
    'FundspBlock': 'FunDSP native block reference',
    'Oddio': 'Oddio reference',
}
SCOPES = {
    'Offline': 'Graph, RAM players, gain/sum and timed command batches.',
    'Direct': 'Purpose-built implementation of this workload.',
    'Fundsp': 'Scalar WavePlayer + gain; adapter scheduling, sum and positions.',
    'FundspBlock': 'Native AudioUnit::process blocks; adapter scheduling and sum. The pinned WavePlayer block implementation uses scalar tick internally.',
    'Oddio': 'Independent Oddio adapter; eligibility requires exact oracle equivalence.',
}
CLI_CANDIDATES = {'offline': 'Offline', 'direct': 'Direct', 'fundsp': 'Fundsp', 'fundsp-block': 'FundspBlock', 'oddio': 'Oddio'}
RESOURCE_COLORS = {'Offline': '#426f9d', 'Direct': '#777777', 'Fundsp': '#94724e', 'FundspBlock': '#756a93', 'Oddio': '#537d79'}
LITERAL_KEYS = {'Direct': 'first_direct', 'Fundsp': 'first_fundsp', 'FundspBlock': 'fundsp_block', 'Oddio': 'oddio'}


def present_candidates(aggregates, extra=()):
    present = {a['candidate'] for a in aggregates} | set(extra)
    return [c for c in NAMES if c in present]


def literal_checks(gates, candidate):
    if candidate == 'Offline':
        return gates.get('literal_offline', [])
    evidence = gates.get(LITERAL_KEYS[candidate], [])
    return evidence.get('cases', []) if candidate in ('Direct', 'Fundsp') and isinstance(evidence, dict) else evidence


def literals_pass(gates, candidate, required_gate_ids=None):
    checks = literal_checks(gates, candidate)
    if candidate in ('FundspBlock','Oddio') and required_gate_ids is not None:
        if not required_gate_ids or not isinstance(checks,list):
            return False
        checks = [g for g in checks if g.get('id') in required_gate_ids]
        if {g.get('id') for g in checks} != set(required_gate_ids):
            return False
    return isinstance(checks, list) and bool(checks) and all(c.get('passed') is True if candidate in ('Offline','FundspBlock','Oddio') else c.get('outcome')=='pass' for c in checks)
