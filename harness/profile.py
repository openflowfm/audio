#!/usr/bin/env python3
"""Separate, bounded diagnostic run. No throughput artifacts are produced."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import time
from resource_sampling import ResourceSampler


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-root', type=Path, required=True)
    parser.add_argument('--workload', default='loop-32')
    parser.add_argument('--candidate', choices=['offline', 'direct', 'fundsp', 'fundsp-block', 'oddio'], default='offline')
    parser.add_argument('--repeats', type=int, default=100000)
    parser.add_argument('--seconds', type=int, default=5)
    parser.add_argument('--enqueue', choices=['scan', 'counted'], default='scan')
    parser.add_argument('--renderer', choices=['generic', 'flat-fused'], default='flat-fused')
    parser.add_argument('--sample', action='store_true', help='macOS sample: retain real stack samples (includes idle stacks)')
    args = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    root = args.output_root.resolve(strict=True)
    if not root.is_dir() or root == repo or repo in root.parents:
        parser.error('output root must be an existing directory outside the repository')
    if not 1 <= args.seconds <= 60 or not 1 <= args.repeats <= 100000:
        parser.error('seconds must be 1..60; repeats must be 1..100000')
    if platform.system() not in ('Darwin', 'Linux'):
        parser.error('launcher currently supports macOS and Linux only')
    os.chdir(repo / 'harness')
    subprocess.run(['cargo', 'build', '--locked', '--profile', 'profiling', '--bin', 'audio-profile'], check=True)
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1']))
    binary = Path(metadata['target_directory']) / 'profiling/audio-profile'
    directory = Path(tempfile.mkdtemp(prefix='audio-profile-', dir=root))
    command = [str(binary), str(directory), args.workload, args.candidate, str(args.repeats), str(args.seconds), args.renderer, args.enqueue]
    (directory / 'source.patch').write_bytes(subprocess.check_output(['git', 'diff', 'HEAD']))
    # Include untracked implementation files as well as tracked source, never old studies or build products.
    import zipfile
    files = subprocess.check_output(['git', '-C', str(repo), 'ls-files', '-co', '--exclude-standard', '-z']).split(b'\0')
    with zipfile.ZipFile(directory / 'source.zip', 'w', zipfile.ZIP_DEFLATED) as archive:
        for name in sorted(set(files)):
            if name and (repo / os.fsdecode(name)).is_file():
                archive.write(repo / os.fsdecode(name), os.fsdecode(name))
    meta = {'command': command, 'platform': platform.platform(), 'machine': platform.machine(),
            'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'sampling': 'not requested'}
    with (directory / 'stderr.txt').open('w') as err, (directory / 'stdout.txt').open('w') as out:
        child = subprocess.Popen(command, stdout=out, stderr=err)
        sampler = None
        resources_sampler = None
        deadline = time.monotonic() + 30
        while not (directory / 'ready').exists() and child.poll() is None and time.monotonic() < deadline:
            time.sleep(.02)
        if (directory / 'ready').exists():
            if args.sample and platform.system() == 'Darwin' and shutil.which('sample'):
                log = (directory / 'sample-launch.txt').open('w')
                sample_cmd = ['sample', str(child.pid), str(args.seconds), '1', '-mayDie', '-file', str(directory / 'stacks.txt')]
                sampler = subprocess.Popen(sample_cmd, stdout=log, stderr=subprocess.STDOUT)
                log.close()
                meta['sampling'] = {'command': sample_cmd, 'scope': 'post-gate process stacks; may include handshake, phase probes, validation and exit'}
            elif args.sample:
                meta['sampling'] = 'unavailable: automatic sampling supports macOS sample only'
            resources_sampler = ResourceSampler(child.pid)
            resources_sampler.start()
            (directory / 'go').touch()
        # wait4 gives this child only, excluding compiler and sampler resource consumption.
        if child.returncode is None:
            _, status, usage = os.wait4(child.pid, 0)
            child.returncode = os.waitstatus_to_exitcode(status)
            meta['process_resources'] = {'scope': 'entire profile child including gates, handshake, fixture generation and artifact writes',
                'user_seconds': usage.ru_utime, 'system_seconds': usage.ru_stime,
                'max_rss_bytes': usage.ru_maxrss * (1 if platform.system() == 'Darwin' else 1024),
                'input_block_operations': usage.ru_inblock, 'output_block_operations': usage.ru_oublock,
                'minor_faults': usage.ru_minflt, 'major_faults': usage.ru_majflt,
                'disk_bytes': None, 'disk_latency': None, 'disk_queue_depth': None,
                'io_scope': 'OS rusage block operation counters; zero may reflect caching or platform accounting, not proof of no I/O'}
        else:
            meta['process_resources'] = None
        if resources_sampler:
            series = resources_sampler.finish()
            (directory / 'resource-samples.json').write_text(json.dumps(series, indent=2) + '\n')
            meta['resource_series'] = {'file': 'resource-samples.json', 'samples': len(series['samples']), 'source': series['source']}
        if sampler:
            meta['sampling']['exit_code'] = sampler.wait(timeout=args.seconds + 30)
            meta['sampling']['available'] = meta['sampling']['exit_code'] == 0 and (directory / 'stacks.txt').exists()
    meta['exit_code'] = child.returncode
    (directory / 'process.json').write_text(json.dumps(meta, indent=2) + '\n')
    lines = ['# Diagnostic profile', '', f'Workload: {args.workload}; candidate: {args.candidate}; exit: {child.returncode}.', '',
        'Instrumentation changes costs. These are not canonical throughput results or realtime guarantees.', '',
        'CPU columns use process CPU clock deltas, including probe overhead; wall time includes descheduling. Stack samples include idle stacks and are not CPU percentages.', '',
        'Allocator metrics cover successful Rust global allocator requests only: realloc counts as a free plus a full new request. Live/peak are requested bytes, not RSS, usable heap size, native malloc, or allocation backtraces. Phase peaks are absolute process live bytes including harness data.', '',
        'Preparation includes asset copy, compilation/adapter setup, state, output buffers and validation capture allocation. Fixture phase builds events and oracle. Render and capture probes run per block. Teardown drops trial data.', '',
        'Process RSS and I/O cover the whole child, including gates and writes. Block operations do not measure disk pressure; disk bytes, latency and queue depth are unavailable. Assets are RAM resident.', '']
    lines += ['Sampling status: ' + json.dumps(meta['sampling']), '']
    if (directory / 'profile.json').exists():
        data = json.loads((directory / 'profile.json').read_text())
        lines += [f"Completed {data['completed']} trials; no warmups; block size 256. Every trial validated.", '',
            '| Phase | Wall ms | CPU ms | Allocations | Requested bytes | Live delta bytes | Peak live bytes |',
            '|---|---:|---:|---:|---:|---:|---:|']
        phases = data['phases']
        phases['artifact_write_profile_json'] = json.loads((directory / 'artifact-writes.json').read_text())
        for name, p in phases.items():
            cpu = 'unavailable' if p['process_cpu_ns'] is None else f"{p['process_cpu_ns']/1e6:.3f}"
            lines.append(f"| {name} | {p['wall_ns']/1e6:.3f} | {cpu} | {p['allocations']} | {p['requested_bytes']} | {p['live_delta_bytes']} | {p['peak_live_bytes']} |")
    lines += ['', 'Artifact-write phase covers profile.json serialization/write only; gates, sidecar and launcher writes are excluded. Raw totals are in profile.json and artifact-writes.json; process counters and sampling status in process.json. Raw sampling output, when available, is stacks.txt; failures are retained in sample-launch.txt. Source snapshot is source.zip.', '', json.dumps(meta.get('process_resources'), indent=2)]
    (directory / 'report.md').write_text('\n'.join(lines) + '\n')
    if child.returncode == 0:
        hashes = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in directory.iterdir() if p.is_file()}
        (directory / 'profile-complete.json').write_text(json.dumps({'kind': 'instrumented-profile', 'sha256': hashes}, indent=2) + '\n')
    print(directory)
    if child.returncode:
        print((directory / 'stderr.txt').read_text())
    return child.returncode


if __name__ == '__main__':
    raise SystemExit(main())
