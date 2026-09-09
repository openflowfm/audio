"""Per-workload actual durations and independent ours-versus-reference comparisons."""
from html import escape as esc
from candidates import NAMES, SCOPES, present_candidates, literal_checks
from impact_charts import canonical_comparison, duration_plot, relative_to_reference


def comparison_table(headers, rows, classes):
    return '<div class="scroll table-scroll"><table class="candidate-table"><thead><tr>'+''.join('<th>'+esc(h)+'</th>' for h in headers)+'</tr></thead><tbody>'+''.join('<tr class="'+esc(css)+'">'+''.join('<td>'+esc(str(value))+'</td>' for value in row)+'</tr>' for row,css in zip(rows,classes))+'</tbody></table></div>'


def eligibility(candidate, aggregate, checks):
    relevant = [c for c in checks if c['candidate'] == candidate]
    if aggregate and aggregate['eligible']:
        return 'Eligible'
    if relevant and all(c['outcome'] == 'capability_gap' for c in relevant):
        return 'Capability gap'
    if any(c['outcome'] == 'fail' for c in relevant):
        return 'Conformance failure'
    return 'Ineligible / unavailable'


def canonical_views(aggregates, checks, timing_scope, preparation_scope, candidates=None, gates=None, native_requirements=()):
    candidates = candidates or present_candidates(aggregates)
    by_candidate = {a['candidate']: a for a in aggregates}
    medians = {c: a['median_ns'] if a['eligible'] else None for c,a in by_candidate.items()}
    observations = {c: a['durations_ns'] if a['eligible'] else [] for c,a in by_candidate.items()}
    milliseconds = lambda value: '—' if value is None else f'{value/1e6:.4f}'
    rows = []
    for c in candidates:
        a = by_candidate.get(c)
        status = eligibility(c,a,checks)
        observed_range = f'{a["min_ns"]/1e6:.4f}–{a["max_ns"]/1e6:.4f}' if a and a['eligible'] and a['min_ns'] is not None else '—'
        rows.append((NAMES[c],status,a['repeats'] if a else 0,milliseconds(medians.get(c)),observed_range))
    classes = [('timing-reference' if c=='Direct' else '')+(' ours-row' if c=='Offline' else '') for c in candidates]
    parts = ['<h4>Actual canonical durations — all candidates</h4><p class="note">Inventory colors do not rank competitors. Direct is a blue reference only; our row is bold. Independent comparisons follow below.</p>',
             comparison_table(['Candidate','Eligibility','Retained trials','Median ms','Min–max ms'],rows,classes),
             duration_plot('Absolute end-to-end duration — ours and references',[(NAMES[c],observations.get(c,[])) for c in candidates], 'Timed: '+timing_scope+'. Preparation excluded: '+preparation_scope+'.', bold_indices=tuple(i for i,c in enumerate(candidates) if c=='Offline'),canonical_candidates=candidates,inventory=True)]
    for reference in candidates:
        if reference in ('Offline','Direct'):
            continue
        pair = ['Offline',reference]
        ranks,notes = canonical_comparison(pair,[medians.get(c) for c in pair],reference)
        relative = relative_to_reference(medians.get('Offline'),medians.get(reference),NAMES[reference])
        parts.append(f'<div class="reference-pair" data-reference="{esc(reference)}"><h4>Ours vs {esc(NAMES[reference])}</h4><p>{esc(SCOPES[reference])}</p>')
        rows = [(NAMES[c],eligibility(c,by_candidate.get(c),checks),milliseconds(medians.get(c)),notes[i],*(relative if c=='Offline' else ('—',)*3)) for i,c in enumerate(pair)]
        parts.append(comparison_table(['Candidate','Eligibility','Median ms','Vs faster in this pair','Ours: time difference','Ours: % time (÷ this reference)','Ours / reference time ratio'],rows,[rank+(' ours-row' if c=='Offline' else '') for c,rank in zip(pair,ranks)]))
        parts.append(duration_plot('Paired reference comparison — uninstrumented',[(NAMES[c],observations.get(c,[])) for c in pair], 'Timed: '+timing_scope+'.',canonical_candidates=pair,comparison_reference=reference,reference_label=NAMES[reference],bold_indices=(0,)))
        required_ids = next((r['gate_ids'] for r in native_requirements if r['candidate']==reference and aggregates and r['workload']==aggregates[0]['workload']),None)
        if required_ids is not None:
            relevant = [g for g in literal_checks(gates or {},reference) if g.get('id') in required_ids]
            parts.append('<p class="note">Required native literal gate IDs: '+esc(', '.join(required_ids) if required_ids else 'none — this workload mode is unsupported')+'. All retained matching partitions must pass before timing.</p>')
            if relevant:
                parts.append(comparison_table(['Native literal gate','Partitions','Outcome','Detail'],[(g['id'],g.get('partitions',[]),'pass' if g['passed'] else 'fail',g.get('detail') or '') for g in relevant],['']*len(relevant)))
        details = [(NAMES[c['candidate']],c.get('block','—'),c['outcome'],c.get('detail') or '') for c in checks if c['candidate'] in pair and c['outcome']!='pass']
        if details:
            parts.append(comparison_table(['Candidate','Block','Gate outcome','Detail'],details,['']*len(details)))
        parts.append('</div>')
    return ''.join(parts)
