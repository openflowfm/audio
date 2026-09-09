#!/usr/bin/env python3
"""Generate a standalone local report from a complete study; never runs audio or timings."""
from impact_charts import duration_plot, duration_ranks, RANK_STYLE, RANK_NOTE, canonical_comparison, relative_to_fundsp
import argparse
import html
import json
from pathlib import Path

NAMES = {"Offline": "audio[flow] MVP (ours)", "Direct": "Direct reference", "Fundsp": "FunDSP reference"}
SCOPES = {
    "Offline": "Graph, command batches, native positions",
    "Direct": "Purpose-built implementation of this workload.",
    "Fundsp": "WavePlayer + native gain; adapter schedule, sum and positions",
}


def checksum(data):
    value = 0xCBF29CE484222325
    for byte in data:
        value = ((value ^ byte) * 0x100000001B3) & ((1 << 64) - 1)
    return value


def read_study(directory):
    marker = json.loads((directory / "complete.json").read_text())
    if marker["schema_version"] != 1:
        raise ValueError("unsupported completion schema")
    for name, size, digest in marker["artifacts"]:
        if Path(name).name != name:
            raise ValueError("invalid artifact name")
        data = (directory / name).read_bytes()
        if len(data) != size or checksum(data) != digest:
            raise ValueError(f"incomplete or modified artifact: {name}")
    required = {"manifest.json", "conformance.json", "workloads.json", "timings.jsonl", "summary.json"}
    if not required.issubset({row[0] for row in marker["artifacts"]}):
        raise ValueError("missing study artifacts")
    manifest = json.loads((directory / "manifest.json").read_text())
    conformance = json.loads((directory / "conformance.json").read_text())
    workloads = json.loads((directory / "workloads.json").read_text())
    trials = [json.loads(line) for line in (directory / "timings.jsonl").read_text().splitlines()]
    summaries = json.loads((directory / "summary.json").read_text())
    if manifest["schema_version"] != 1 or manifest["kind"] != "offline-study":
        raise ValueError("not a supported offline study")
    if conformance["schema_version"] != 1 or summaries["schema_version"] != 1:
        raise ValueError("unsupported evidence schema")
    workload_map = {w["id"]: w for w in workloads}
    if len(workload_map) != len(workloads):
        raise ValueError("duplicate workload identity")
    pairs = {(a["workload"], a["candidate"]) for a in summaries["aggregates"]}
    if len(pairs) != len(summaries["aggregates"]):
        raise ValueError("duplicate aggregate identity")
    integer_fields = ("trial", "order", "elapsed_ns", "preparation_ns", "frames", "sample_count", "block")
    orders = set()
    for trial in trials:
        if (trial["workload"], trial["candidate"]) not in pairs or trial["candidate"] not in NAMES:
            raise ValueError("orphan or unknown trial")
        if trial["schema_version"] != 1 or any(type(trial[key]) is not int or not 0 <= trial[key] <= (1 << 64)-1 for key in integer_fields):
            raise ValueError("invalid integer trial metadata")
        if trial["frames"] != workload_map[trial["workload"]]["frames"] or trial["sample_count"] != trial["frames"] or trial["frames"] == 0 or trial["block"] != manifest["timing_block"]:
            raise ValueError("trial changed sample count or timing block")
        if trial["order"] in orders:
            raise ValueError("duplicate execution order")
        orders.add(trial["order"])
    for aggregate in summaries["aggregates"]:
        pair = (aggregate["workload"], aggregate["candidate"])
        matching = [t for t in trials if (t["workload"], t["candidate"]) == pair]
        if aggregate["eligible"]:
            checks = [c for c in conformance["checks"] if (c["workload"], c["candidate"]) == pair]
            literal = conformance["literal_offline"] if pair[1] == "Offline" else conformance["first_direct" if pair[1] == "Direct" else "first_fundsp"]["cases"]
            literals_pass = bool(literal) and all(c["passed"] if pair[1] == "Offline" else c["outcome"] == "pass" for c in literal)
            if not literals_pass or sorted(c["block"] for c in checks) != sorted(manifest["conformance_blocks"]) or any(c["outcome"] != "pass" for c in checks):
                raise ValueError("performance claimed without passing correctness gates")
            if any((e["workload"], e["candidate"]) == pair for e in conformance["execution_errors"]):
                raise ValueError("performance claimed for a failed execution")
            if len(matching) != manifest["repeats"] or aggregate["repeats"] != len(matching) or [t["trial"] for t in matching] != list(range(manifest["repeats"])):
                raise ValueError("missing or duplicate retained trials")
            if not matching or any(t["source"] != "monotonic" or t["conformance"] != "pass" for t in matching):
                raise ValueError("nonconforming or synthetic speed result")
            if len({t["scope"] for t in matching}) != 1:
                raise ValueError("mixed timing scopes")
            durations = [t["elapsed_ns"] for t in matching]
            expected = (sum(durations), sum(t["frames"] for t in matching), min(durations), max(durations), sorted(durations)[(len(durations)-1)//2])
            actual = tuple(aggregate[key] for key in ("total_ns", "total_frames", "min_ns", "max_ns", "median_ns"))
            if expected != actual or durations != aggregate["durations_ns"] or [t["preparation_ns"] for t in matching] != aggregate["preparation_ns"]:
                raise ValueError("derived summary disagrees with raw timings")
        elif aggregate["total_ns"] or aggregate["total_frames"] or aggregate["durations_ns"] or aggregate["preparation_ns"] or any(aggregate[key] is not None for key in ("min_ns", "max_ns", "median_ns")):
            raise ValueError("ineligible workload has comparative metrics")
    return manifest, conformance, workloads, trials, summaries["aggregates"]


def esc(value):
    return html.escape(str(value))


def milliseconds(ns):
    return f"{ns / 1_000_000:,.4f}"


def plot(aggregates, title):
    return duration_plot(title, [(NAMES[a['candidate']], a['durations_ns'] if a['eligible'] else []) for a in aggregates], 'Uninstrumented canonical durations.', bold_indices=tuple(i for i,a in enumerate(aggregates) if a['candidate']=='Offline'), canonical_candidates=[a['candidate'] for a in aggregates])


def build_report(directory):
    manifest, conformance, workloads, trials, summaries = read_study(directory)
    counts = {key: sum(c["outcome"] == key for c in conformance["checks"]) for key in ("pass", "fail", "capability_gap")}
    literal = conformance["literal_offline"]
    first_cases = conformance["first_direct"]["cases"] + conformance["first_fundsp"]["cases"]
    buffer_sizes = sorted({t["output_buffer_bytes"] for t in trials})
    buffer_label = f"{buffer_sizes[0]:,}–{buffer_sizes[-1]:,} bytes" if buffer_sizes else "unavailable (no timed trials)"
    sections = []
    for w in workloads:
        event_label = f" · {len(w['events']):,} batches / {sum(len(b['commands']) for b in w['events']):,} commands" if 'events' in w else ''
        rows = [a for a in summaries if a["workload"] == w["id"]]
        table = []
        ranks, comparison_notes = canonical_comparison([a['candidate'] for a in rows], [a['median_ns'] if a['eligible'] else None for a in rows])
        canonical_medians = {a['candidate']: a['median_ns'] for a in rows if a['eligible']}
        relative = relative_to_fundsp(canonical_medians.get('Offline'), canonical_medians.get('Fundsp'))
        for a, rank, comparison_note in zip(rows, ranks, comparison_notes):
            checks = [c for c in conformance["checks"] if c["workload"] == w["id"] and c["candidate"] == a["candidate"]]
            status = "pass" if all(c["outcome"] == "pass" for c in checks) else "; ".join(sorted({c["outcome"] for c in checks}))
            if a["eligible"]:
                prep = sorted(a["preparation_ns"])[(len(a["preparation_ns"])-1)//2]
                throughput = a["total_frames"] * 1_000_000_000 / a["total_ns"] if a["total_ns"] else 0
                metrics = f'<td>{milliseconds(a["median_ns"])}</td><td>{milliseconds(a["min_ns"])}–{milliseconds(a["max_ns"])}</td><td>{throughput:,.0f}</td><td>{milliseconds(prep)}</td>'
            else:
                reason = next((c["detail"] for c in checks if c["detail"]), "Excluded by correctness gate")
                metrics = f'<td colspan="4">{esc(reason)}</td>'
            row_class = rank + (' ours-row' if a['candidate'] == 'Offline' else '')
            relative_cells = ''.join('<td>'+esc(v)+'</td>' for v in (relative if a['candidate']=='Offline' else ('—',)*3))
            table.append(f'<tr class="{row_class}"><th scope="row">{esc(NAMES[a["candidate"]])}</th><td>{esc(status)}</td>{metrics}<td>{esc(comparison_note)}</td>{relative_cells}</tr>')
        selected = [t for t in trials if t["workload"] == w["id"]]
        rawrows = "".join(f'<tr><td>{esc(NAMES[t["candidate"]])}</td><td>{t["trial"]+1}</td><td>{t["order"]}</td><td>{t["elapsed_ns"]}</td><td>{t["preparation_ns"]}</td><td>{t["frames"]}</td></tr>' for t in selected)
        sections.append(f'''<section id="{esc(w['id'])}"><h2>{esc(w['id'])}</h2>
<p>{w['voices']} independent voices · {w['frames']:,} mono frames ({w['frames']/48000:.3f} s of audio) · seed {w['seed']} · {esc(w['mode'])}{event_label}{f" · burst period {w['period']} frames" if w['mode']=='Bursts' else ''}</p>
{plot(rows,w['id'])}<div class="table-scroll"><table><thead><tr><th>Candidate</th><th>Conformance</th><th>Median ms</th><th>Observed range ms</th><th>Frames/s</th><th>Prep median ms</th><th>Vs faster (ours / FunDSP)</th><th>Ours vs FunDSP: time difference</th><th>Ours vs FunDSP: % time (÷ FunDSP)</th><th>Ours / FunDSP time ratio</th></tr></thead><tbody>{''.join(table)}</tbody></table></div>
<details><summary>Retained raw trials for this workload</summary><div class="table-scroll"><table><thead><tr><th>Candidate</th><th>Trial</th><th>Execution order</th><th>Render ns</th><th>Preparation ns</th><th>Frames</th></tr></thead><tbody>{rawrows}</tbody></table></div></details></section>''')
    links = " · ".join(f'<a href="{name}">{label}</a>' for name,label in [("manifest.json","Environment"),("workloads.json","Workloads and seeds"),("conformance.json","Conformance evidence"),("timings.jsonl","Raw integer timings"),("summary.json","Derived summary"),("complete.json","Completion checksums")])
    scopes = "".join(f'<tr><th>{esc(NAMES[key])}</th><td>{esc(scope)}</td></tr>' for key,scope in SCOPES.items())
    navigation = " · ".join(f'<a href="#{esc(w["id"])}">{esc(w["id"])}</a>' for w in workloads)
    limitations = "".join(f'<li>{esc(item)}</li>' for item in manifest["limitations"])
    gates = "".join(f'<li>{esc(g["id"])} · blocks {esc(g.get("partitions", []))}: {"pass" if g["passed"] else "FAIL"}</li>' for g in literal)
    return f'''<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>audio[flow] — local offline evidence</title>
<style>
:root{{color-scheme:light dark;--ink:light-dark(#17252a,#dce5e7);--muted:light-dark(#52666d,#a8b8bd);--paper:light-dark(#f8faf9,#132025);--line:light-dark(#cedbd8,#3b5057);--a:light-dark(#176c63,#69c4b7);--b:light-dark(#90551d,#e5b06c);--c:light-dark(#654e99,#b5a0e0)}}
*{{box-sizing:border-box}}body{{margin:0;background:var(--paper);color:var(--ink);font:16px/1.6 system-ui,sans-serif}}main{{max-width:1100px;margin:auto;padding:40px 24px 80px}}h1{{font-size:2.15rem;margin:.2em 0}}h2{{font-size:1.45rem;margin:0}}p{{max-width:90ch}}a{{color:var(--a);overflow-wrap:anywhere}}.eyebrow{{letter-spacing:.12em;text-transform:uppercase;color:var(--muted);font-size:.8rem}}section{{padding:30px 0;border-top:1px solid var(--line)}}nav{{padding:16px 0 30px}}table{{border-collapse:collapse;width:100%;font-size:.9rem}}th,td{{text-align:left;padding:10px 12px;border-bottom:1px solid var(--line);font-variant-numeric:tabular-nums}}th{{font-weight:600}}.table-scroll{{overflow:auto}}svg{{width:100%;height:auto;max-width:900px;display:block;margin:20px 0}}svg text{{font:13px system-ui;fill:var(--ink)}}.grid{{stroke:var(--line);stroke-width:1}}.point{{stroke:var(--paper);stroke-width:1}}.offline{{fill:var(--a)}}.direct{{fill:var(--b)}}.fundsp{{fill:var(--c)}}details{{margin-top:20px}}summary{{cursor:pointer;color:var(--a)}}code{{overflow-wrap:anywhere}}.note{{color:var(--muted)}}@media(max-width:600px){{main{{padding:24px 16px}}h1{{font-size:1.7rem}}svg{{min-width:580px}}section>svg{{max-width:none}}.plot{{overflow:auto}}}}
{RANK_STYLE}
tbody th{{font-weight:inherit}}
</style></head><body><main><div class="eyebrow">audio[flow] · local evidence · schema 1</div><h1>Offline rendering comparison</h1>
<p>{counts['pass']} seeded partition checks passed; {counts['capability_gap']} capability gaps; {counts['fail']} failed. {sum(g['passed'] for g in literal)}/{len(literal)} literal offline gates and {sum(c['outcome']=='pass' for c in first_cases)}/{len(first_cases)} first-slice adapter checks passed. {len(conformance['execution_errors'])} execution errors. {len(trials)} real monotonic trials retained.</p>
<p>Compare costs within each workload. Direct reference is a purpose-built implementation of this workload; the offline MVP supports a broader graph and command contract. FunDSP uses its scalar API here. These measurements do not establish a universal winner or realtime reliability.</p>
<p>{RANK_NOTE}</p><p>{links}</p><nav aria-label="Workloads">{navigation}</nav>
<section><h2>What was measured</h2><table><tbody>{scopes}</tbody></table>
<p><strong>Render boundary:</strong> {esc(manifest['timing_scope'])}. <strong>Preparation:</strong> {esc(manifest['preparation_scope'])}.</p>
<p>48 kHz · mono f32 · {manifest['timing_block']}-frame timed blocks · correctness blocks {esc(manifest['conformance_blocks'])}. Three warmups, then twelve repeats per supported pair, rotating candidate order. Full validation and position observation occur outside timing. Frames/s is total frames divided by total measured seconds; medians use the lower middle observation for even counts. Dots show every retained duration, with min–max variability; no confidence bound is inferred.</p>
<p><strong>Memory:</strong> {esc(manifest['memory_scope'])}. The fixture stores 1,028 sample bytes; prepared output buffers use {buffer_label} across these workloads. Those logical byte counts are not total process or allocator memory.</p>
</section>{''.join(sections)}
<section><h2>Environment and limitations</h2><p>{esc(manifest['hardware'] or 'Hardware model unavailable')} · {esc(manifest['os'] or 'OS unavailable')} · {esc(manifest['compiler'])} · {esc(manifest['target'])} · {esc(manifest['profile'])}, optimization {esc(manifest['opt_level'])}</p>
<p>Revision at run: <code>{esc(manifest['revision_at_run'])}</code>. Dirty checkout status is retained in the manifest. Exact dependencies are recorded by the embedded lockfile; FunDSP is pinned to 0.20.0 under its MIT option.</p><ul>{limitations}</ul>
<details><summary>Independent literal gates</summary><ul>{gates}</ul><p>Fault-injection and allocation tests are separate verification commands; they are not performance samples.</p></details>
<p class="note">Generated solely from the complete retained run at <code>{esc(directory)}</code>. No network assets or publication.</p></section></main></body></html>'''.replace('<svg viewBox=', '<div class="plot"><svg viewBox=').replace('</svg>', '</svg></div>')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run_directory",type=Path)
    args = parser.parse_args()
    directory = args.run_directory.resolve()
    document = build_report(directory)
    target = directory / "report.html"
    with target.open("x") as output:
        output.write(document)
    print(target)


if __name__ == "__main__":
    main()
