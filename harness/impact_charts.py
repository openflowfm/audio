"""Measured timing ranks and separate neutral SVG views of retained timings and separately instrumented CPU costs."""
from html import escape as esc


RANK_COLORS = {'timing-low': '#25834b', 'timing-middle': '#c69a13', 'timing-high': '#c34141', 'timing-reference': '#2875bf', '': '#536a80'}
SLOWER_YELLOW_PERCENT = 10
RANK_STYLE = '.timing-reference text{fill:#205a96}.impact svg g[font-weight="700"] text{font-weight:700}.timing-reference{background:#e4effb;color:#205a96}.timing-low{background:#e1f5e6;color:#18552c}.timing-middle{background:#fff2bf;color:#695100}.timing-high{background:#fbe7e7;color:#852d2d}.ours-row{font-weight:700}'
RANK_NOTE = f'Canonical timing compares only audio[flow] and FunDSP: faster green; slower yellow up to {SLOWER_YELLOW_PERCENT}% slower, red above {SLOWER_YELLOW_PERCENT}%. Percentages use the faster eligible median. Exact ties and missing comparisons stay neutral. Direct reference is always blue and excluded from this comparison. Our row is bold. Controlled strategy comparisons remain separate: lower median green, higher red within each replicate; ties neutral. Diagnostic resources stay neutral.'


def canonical_comparison(candidates, values):
    """Compare only eligible Offline/Fundsp medians; Direct never sets the benchmark."""
    medians = dict(zip(candidates, values))
    ranks = {c: 'timing-reference' if c == 'Direct' else '' for c in candidates}
    notes = {c: 'Reference only' if c == 'Direct' else 'No comparison' for c in candidates}
    ours, fundsp = medians.get('Offline'), medians.get('Fundsp')
    if ours is not None and fundsp is not None:
        if ours == fundsp:
            for c in ('Offline', 'Fundsp'):
                notes[c] = 'Tied (0.00%)'
        else:
            fast, slow = ('Offline', 'Fundsp') if ours < fundsp else ('Fundsp', 'Offline')
            faster, slower = medians[fast], medians[slow]
            ranks[fast] = 'timing-low'
            notes[fast] = '0.00% vs faster'
            ranks[slow] = 'timing-middle' if (slower-faster)*100 <= faster*SLOWER_YELLOW_PERCENT else 'timing-high'
            notes[slow] = f'{100*(slower-faster)/faster:.2f}% slower' if faster else 'vs zero: undefined %'
    return [ranks[c] for c in candidates], [notes[c] for c in candidates]



def duration_unit(ns):
    """Readable absolute duration with no fixed-unit rounding to zero."""
    value = abs(ns)
    for scale, unit in ((1_000_000_000, 's'), (1_000_000, 'ms'), (1_000, 'µs')):
        if value >= scale:
            return f'{value/scale:.4g} {unit}'
    return f'{value:.4g} ns'


def relative_to_fundsp(ours, fundsp):
    """Ours-minus-reference direction; percentages and time ratio use FunDSP."""
    if ours is None or fundsp is None:
        return ('Unavailable',) * 3
    difference = ours-fundsp
    direction = 'less' if difference < 0 else 'more'
    delta = 'Tied (0 ns)' if difference == 0 else f'{duration_unit(difference)} {direction}'
    if fundsp == 0:
        return delta, 'Undefined (FunDSP = 0)', 'Undefined (FunDSP = 0)'
    percent = 'Tied (0.00%)' if difference == 0 else f'{100*abs(difference)/fundsp:.2f}% {direction}'
    return delta, percent, f'{ours/fundsp:.4g}×'


def duration_ranks(values):
    valid = [v for v in values if v is not None]
    if not valid or min(valid) == max(valid):
        return [''] * len(values)
    return ['' if v is None else 'timing-low' if v == min(valid) else 'timing-high' if v == max(valid) else 'timing-middle' for v in values]


def duration_plot(title, rows, scope, comparison_groups=None, bold_indices=(), canonical_candidates=None):
    """Rows are (label, retained integer ns); empty observations remain gaps."""
    medians = [sorted(values)[(len(values)-1)//2] if values else None for _, values in rows]
    groups = comparison_groups if comparison_groups is not None else [0] * len(rows)
    ranks = [''] * len(rows)
    for group in dict.fromkeys(groups):
        indices = [i for i, g in enumerate(groups) if g == group]
        for i, rank in zip(indices, duration_ranks([medians[i] for i in indices])):
            ranks[i] = rank
    notes = None
    relative_note = ''
    if canonical_candidates is not None:
        ranks, notes = canonical_comparison(canonical_candidates, medians)
        canonical = dict(zip(canonical_candidates, medians))
        delta, percent, ratio = relative_to_fundsp(canonical.get('Offline'), canonical.get('Fundsp'))
        relative_note = f' Ours vs FunDSP: {delta} time; {percent} time (FunDSP denominator); ours/FunDSP time ratio {ratio}.'
    maximum = max((v / 1e6 for _, values in rows for v in values), default=0) or 1
    maximum *= 1.08
    height = 90 + 76 * len(rows)
    parts = [f'<figure class="impact"><h4>{esc(title)}</h4><svg viewBox="0 0 1000 {height}" role="img"><title>{esc(title)} — retained trials and lower-middle medians</title>']
    for step in range(5):
        x = 300 + 480 * step / 4
        parts.append(f'<path d="M{x} 18V{height-48}" stroke="#dce3e9"/><text x="{x}" y="{height-27}" text-anchor="middle">{maximum*step/4:.4g}</text>')
    for i, (label, values) in enumerate(rows):
        y = 42 + i * 76
        color = RANK_COLORS[ranks[i]]
        weight = '700' if i in bold_indices else '400'
        parts.append(f'<g class="{ranks[i]}" data-rank="{ranks[i]}" font-weight="{weight}">')
        parts.append(f'<text x="12" y="{y}">{esc(label)}</text><text x="12" y="{y+20}">n = {len(values)}</text>')
        if not values:
            parts.append(f'<text x="310" y="{y}">Unavailable / capability gap</text>')
            parts.append('</g>')
            continue
        median = sorted(values)[(len(values)-1)//2] / 1e6
        parts.append(f'<path d="M300 {y}H{300+480*median/maximum:.3f}" stroke="{color}" stroke-opacity="0.35" stroke-width="12"/>')
        for j, value in enumerate(values):
            x = 300 + 480 * value / 1e6 / maximum
            parts.append(f'<circle cx="{x:.3f}" cy="{y+(j%5-2)*4}" r="3" fill="{color}"><title>{esc(label)} trial {j+1}: {value/1e6:.6f} ms</title></circle>')
        x = 300 + 480 * median / maximum
        parts.append(f'<path d="M{x:.3f} {y-15}v30" stroke="{color}" stroke-width="3"/><text x="810" y="{y}">{median:.4f} ms median</text>')
        if notes is not None:
            parts.append(f'<text x="810" y="{y+22}">{esc(notes[i])}</text>')
        parts.append('</g>')
    parts.append(f'<text x="540" y="{height-5}" text-anchor="middle">Complete workload elapsed time (ms) · zero origin</text></svg><figcaption>Dots: retained trials; dark tick: lower-middle median; bar: zero to median. Shared scale within this workload comparison. {"Percentages compare only ours and FunDSP against their faster eligible median. " if canonical_candidates is not None else ""}{esc(scope)}{esc(relative_note)}</figcaption></figure>')
    return ''.join(parts)


def paired_plot(data, workload, strategies=None):
    rows, deltas = [], []
    strategies = strategies or ['Generic', 'FlatFused']
    for rep in range(data['replicates']):
        medians = []
        for strategy in strategies:
            values = [r['trial']['elapsed_ns'] for r in data['trials'] if r['replicate'] == rep and r['strategy'] == strategy and r['trial']['workload'] == workload]
            rows.append((f'Replicate {rep+1} · {strategy}', values))
            medians.append(sorted(values)[(len(values)-1)//2] if values else None)
        if len(medians) == 2 and all(v is not None for v in medians):
            before, after = medians
            saved = before-after
            percent = f'{100*saved/before:.2f}%' if before else 'percentage unavailable (zero control)'
            deltas.append(f'<li>Replicate {rep+1}: {saved/1e6:.4f} ms saved ({percent}); {esc(strategies[0])} minus {esc(strategies[1])} median.</li>')
    scopes = sorted({r['trial']['scope'] for r in data['trials'] if r['trial']['workload'] == workload})
    return duration_plot('audio[flow] stage comparison — uninstrumented', rows, 'Timed scope: '+ '; '.join(scopes), comparison_groups=[rep for rep in range(data['replicates']) for _ in strategies]) + '<ul class="deltas">'+''.join(deltas)+'</ul><p class="note">Differences compare medians within each replicate, not paired-trial difference medians. Negative saved values mean increased time. Replicates share an axis; no cross-run pairing is inferred.</p>'


def phase_plot(measured, names):
    phases = ('enqueue', 'render', 'capture')
    colors = ('#536a80', '#9dacc0', '#cbd4df')
    rows = []
    for candidate, evidence in measured:
        denominator = evidence['completed'] * evidence['workload'].get('frames', 0)
        values = [evidence['phases'].get(p, {}).get('process_cpu_ns') for p in phases]
        values = [v / denominator if v is not None and denominator else None for v in values]
        rows.append((names[candidate] + (' · '+ evidence.get('render_strategy', 'Unrecorded strategy')+' / '+evidence.get('enqueue_strategy', 'unrecorded enqueue') if candidate == 'Offline' else ''), values))
    maximum = max((sum(v for v in values if v is not None) for _, values in rows), default=0) or 1
    height = 120 + 76 * len(rows)
    parts = [f'<figure><h4>Timed-scope phase breakdown — diagnostic CPU</h4><svg viewBox="0 0 1000 {height}" role="img"><title>Instrumented phase CPU ns per output frame</title>']
    for i, (label, values) in enumerate(rows):
        y = 35 + 76*i
        parts.append(f'<text x="12" y="{y-14}">{esc(label.split(" · " )[0])}</text>')
        if ' · ' in label:
            parts.append(f'<text x="12" y="{y+7}">{esc(label.split(" · ", 1)[1])}</text>')
        x = 350
        for phase, value, color in zip(phases, values, colors):
            if value is None:
                continue
            width = 440*value/maximum
            parts.append(f'<rect x="{x:.3f}" y="{y-14}" width="{width:.3f}" height="22" fill="{color}"><title>{esc(label)} {phase}: {value:.3f} CPU ns/output frame</title></rect>')
            x += width
        total = f'{sum(values):.3f}' if all(v is not None for v in values) else 'Partial / unavailable'
        parts.append(f'<text x="805" y="{y}">{total}</text><text x="350" y="{y+29}">'+esc(' · '.join(f'{p}: {v:.3f}' if v is not None else f'{p}: unavailable' for p,v in zip(phases,values)))+'</text>')
    y = height-42
    parts.append(f'<path d="M350 {y}H790" stroke="#536a80"/><text x="350" y="{y+20}">0</text><text x="790" y="{y+20}" text-anchor="end">{maximum:.3f} CPU ns/output frame</text></svg><figcaption>Segments, left to right: enqueue, render, capture. Aggregate process CPU divided by validated trials × output frames; shared zero-based scale. Missing phases remain unavailable; partial bars are not totals. These instrumented phase means are separate from uninstrumented wall-time distributions above. Fixture/oracle, preparation, validation and teardown appear separately below.</figcaption></figure>')
    return ''.join(parts)
