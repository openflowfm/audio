#!/usr/bin/env python3
"""Generate a standalone local report from a complete study; never runs audio or timings."""
import argparse
import html
import json
from pathlib import Path

NAMES = {"Offline": "Offline MVP", "Direct": "Direct loop", "Fundsp": "FunDSP 0.20.0 + adapter"}
SCOPES = {
    "Offline": "Graph, command batches, native positions",
    "Direct": "Specialized flat mixer; adapter scheduled starts",
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
    for aggregate in summaries["aggregates"]:
        matching = [t for t in trials if t["workload"] == aggregate["workload"] and t["candidate"] == aggregate["candidate"]]
        if aggregate["eligible"]:
            if not matching or any(t["source"] != "monotonic" or t["conformance"] != "pass" for t in matching):
                raise ValueError("nonconforming or synthetic speed result")
            durations = [t["elapsed_ns"] for t in matching]
            if any(type(n) is not int or n < 0 for n in durations):
                raise ValueError("non-integer timing")
            expected = (sum(durations), sum(t["frames"] for t in matching), min(durations), max(durations), sorted(durations)[(len(durations)-1)//2])
            actual = tuple(aggregate[key] for key in ("total_ns", "total_frames", "min_ns", "max_ns", "median_ns"))
            if expected != actual or durations != aggregate["durations_ns"]:
                raise ValueError("derived summary disagrees with raw timings")
    return manifest, conformance, workloads, trials, summaries["aggregates"]


def esc(value):
    return html.escape(str(value))


def milliseconds(ns):
    return f"{ns / 1_000_000:,.4f}"


def plot(aggregates, title):
    usable = [a for a in aggregates if a["eligible"]]
    if not usable:
        return "<p>No conforming timed trials.</p>"
    maximum = max(a["max_ns"] for a in usable) or 1
    elements = [f'<svg viewBox="0 0 760 {70+len(usable)*58}" role="img" aria-label="{esc(title)}: individual render durations"><title>{esc(title)} — measured render durations</title>']
    for tick in range(5):
        x = 212 + tick * 126
        elements += [f'<line class="grid" x1="{x}" y1="12" x2="{x}" y2="{len(usable)*58+20}"/>', f'<text x="{x}" y="{len(usable)*58+43}" text-anchor="middle">{milliseconds(maximum*tick/4)}</text>']
    for row, a in enumerate(usable):
        y = 38 + row*58
        elements.append(f'<text x="8" y="{y+4}">{esc(NAMES[a["candidate"]])}</text>')
        for index, ns in enumerate(a["durations_ns"]):
            x = 212 + ns / maximum * 504
            jitter = ((index % 3)-1)*7
            elements.append(f'<circle class="point {a["candidate"].lower()}" cx="{x:.2f}" cy="{y+jitter}" r="4"><title>Trial {index+1}: {ns:,} ns ({milliseconds(ns)} ms)</title></circle>')
    elements.append(f'<text x="464" y="{len(usable)*58+65}" text-anchor="middle">End-to-end duration (ms), 12 retained trials</text></svg>')
    return "".join(elements)


def build_report(directory):
    manifest, conformance, workloads, trials, summaries = read_study(directory)
    counts = {key: sum(c["outcome"] == key for c in conformance["checks"]) for key in ("pass", "fail", "capability_gap")}
    literal = conformance["literal_offline"]
    first_cases = conformance["first_direct"]["cases"] + conformance["first_fundsp"]["cases"]
    sections = []
    for w in workloads:
        rows = [a for a in summaries if a["workload"] == w["id"]]
        table = []
        for a in rows:
            checks = [c for c in conformance["checks"] if c["workload"] == w["id"] and c["candidate"] == a["candidate"]]
            status = "pass" if all(c["outcome"] == "pass" for c in checks) else "; ".join(sorted({c["outcome"] for c in checks}))
            if a["eligible"]:
                prep = sorted(a["preparation_ns"])[(len(a["preparation_ns"])-1)//2]
                throughput = a["total_frames"] * 1_000_000_000 / a["total_ns"] if a["total_ns"] else 0
                metrics = f'<td>{milliseconds(a["median_ns"])}</td><td>{milliseconds(a["min_ns"])}–{milliseconds(a["max_ns"])}</td><td>{throughput:,.0f}</td><td>{milliseconds(prep)}</td>'
            else:
                reason = next((c["detail"] for c in checks if c["detail"]), "Excluded by correctness gate")
                metrics = f'<td colspan="4">{esc(reason)}</td>'
            table.append(f'<tr><th scope="row">{esc(NAMES[a["candidate"]])}</th><td>{esc(status)}</td>{metrics}</tr>')
        selected = [t for t in trials if t["workload"] == w["id"]]
        rawrows = "".join(f'<tr><td>{esc(NAMES[t["candidate"]])}</td><td>{t["trial"]+1}</td><td>{t["order"]}</td><td>{t["elapsed_ns"]}</td><td>{t["preparation_ns"]}</td><td>{t["frames"]}</td></tr>' for t in selected)
        sections.append(f'''<section id="{esc(w['id'])}"><h2>{esc(w['id'])}</h2>
<p>{w['voices']} independent voices · {w['frames']:,} mono frames ({w['frames']/48000:.3f} s of audio) · seed {w['seed']} · {esc(w['mode'])}{f" · burst period {w['period']} frames" if w['mode']=='Bursts' else ''}</p>
{plot(rows,w['id'])}<div class="table-scroll"><table><thead><tr><th>Candidate</th><th>Conformance</th><th>Median ms</th><th>Observed range ms</th><th>Frames/s</th><th>Prep median ms</th></tr></thead><tbody>{''.join(table)}</tbody></table></div>
<details><summary>Retained raw trials for this workload</summary><div class="table-scroll"><table><thead><tr><th>Candidate</th><th>Trial</th><th>Execution order</th><th>Render ns</th><th>Preparation ns</th><th>Frames</th></tr></thead><tbody>{rawrows}</tbody></table></div></details></section>''')
    links = " · ".join(f'<a href="{name}">{label}</a>' for name,label in [("manifest.json","Environment"),("workloads.json","Workloads and seeds"),("conformance.json","Conformance evidence"),("timings.jsonl","Raw integer timings"),("summary.json","Derived summary"),("complete.json","Completion checksums")])
    scopes = "".join(f'<tr><th>{esc(NAMES[key])}</th><td>{esc(scope)}</td></tr>' for key,scope in SCOPES.items())
    navigation = " · ".join(f'<a href="#{esc(w["id"])}">{esc(w["id"])}</a>' for w in workloads)
    limitations = "".join(f'<li>{esc(item)}</li>' for item in manifest["limitations"])
    gates = "".join(f'<li>{esc(g["id"])}: {"pass" if g["passed"] else "FAIL"}</li>' for g in literal)
    return f'''<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>audio[flow] — local offline evidence</title>
<style>
:root{{color-scheme:light dark;--ink:light-dark(#17252a,#dce5e7);--muted:light-dark(#52666d,#a8b8bd);--paper:light-dark(#f8faf9,#132025);--line:light-dark(#cedbd8,#3b5057);--a:light-dark(#176c63,#69c4b7);--b:light-dark(#90551d,#e5b06c);--c:light-dark(#654e99,#b5a0e0)}}
*{{box-sizing:border-box}}body{{margin:0;background:var(--paper);color:var(--ink);font:16px/1.6 system-ui,sans-serif}}main{{max-width:1100px;margin:auto;padding:40px 24px 80px}}h1{{font-size:2.15rem;margin:.2em 0}}h2{{font-size:1.45rem;margin:0}}p{{max-width:90ch}}a{{color:var(--a);overflow-wrap:anywhere}}.eyebrow{{letter-spacing:.12em;text-transform:uppercase;color:var(--muted);font-size:.8rem}}section{{padding:30px 0;border-top:1px solid var(--line)}}nav{{padding:16px 0 30px}}table{{border-collapse:collapse;width:100%;font-size:.9rem}}th,td{{text-align:left;padding:10px 12px;border-bottom:1px solid var(--line);font-variant-numeric:tabular-nums}}th{{font-weight:600}}.table-scroll{{overflow:auto}}svg{{width:100%;height:auto;max-width:900px;display:block;margin:20px 0}}svg text{{font:13px system-ui;fill:var(--ink)}}.grid{{stroke:var(--line);stroke-width:1}}.point{{stroke:var(--paper);stroke-width:1}}.offline{{fill:var(--a)}}.direct{{fill:var(--b)}}.fundsp{{fill:var(--c)}}details{{margin-top:20px}}summary{{cursor:pointer;color:var(--a)}}code{{overflow-wrap:anywhere}}.note{{color:var(--muted)}}@media(max-width:600px){{main{{padding:24px 16px}}h1{{font-size:1.7rem}}svg{{min-width:580px}}section>svg{{max-width:none}}.plot{{overflow:auto}}}}
</style></head><body><main><div class="eyebrow">audio[flow] · local evidence · schema 1</div><h1>Offline rendering comparison</h1>
<p>{counts['pass']} seeded partition checks passed; {counts['capability_gap']} capability gaps; {counts['fail']} failed. {sum(g['passed'] for g in literal)}/{len(literal)} literal offline gates and {sum(c['outcome']=='pass' for c in first_cases)}/{len(first_cases)} first-slice adapter checks passed. {len(conformance['execution_errors'])} execution errors. {len(trials)} real monotonic trials retained.</p>
<p>Compare costs within each workload. The direct loop is a specialized reference; the offline MVP supports a broader graph and command contract. FunDSP uses its scalar API here. These measurements do not establish a universal winner or realtime reliability.</p>
<p>{links}</p><nav aria-label="Workloads">{navigation}</nav>
<section><h2>What was measured</h2><table><tbody>{scopes}</tbody></table>
<p><strong>Render boundary:</strong> {esc(manifest['timing_scope'])}. <strong>Preparation:</strong> {esc(manifest['preparation_scope'])}.</p>
<p>48 kHz · mono f32 · {manifest['timing_block']}-frame timed blocks · correctness blocks {esc(manifest['conformance_blocks'])}. Three warmups, then twelve repeats per supported pair, rotating candidate order. Full validation and position observation occur outside timing. Frames/s is total frames divided by total measured seconds; medians use the lower middle observation for even counts. Dots show every retained duration, with min–max variability; no confidence bound is inferred.</p>
<p><strong>Memory:</strong> {esc(manifest['memory_scope'])}. The fixture stores 1,028 sample bytes; prepared output buffers use {min(t["output_buffer_bytes"] for t in trials):,}–{max(t["output_buffer_bytes"] for t in trials):,} bytes across these workloads. Those logical byte counts are not total process or allocator memory.</p>
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
