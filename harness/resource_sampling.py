"""Read-only process RSS and accounted I/O sampling for diagnostic children."""
import ctypes
import os
from pathlib import Path
import platform
import threading
import time


class RusageV2(ctypes.Structure):
    # SDK sys/resource.h: rusage_info_v2; UUID plus 18 uint64_t fields.
    _fields_ = [('uuid', ctypes.c_uint8 * 16)] + [(name, ctypes.c_uint64) for name in (
        'user_time', 'system_time', 'pkg_idle_wkups', 'interrupt_wkups', 'pageins',
        'wired_size', 'resident_size', 'phys_footprint', 'proc_start_abstime',
        'proc_exit_abstime', 'child_user_time', 'child_system_time', 'child_pkg_idle_wkups',
        'child_interrupt_wkups', 'child_pageins', 'child_elapsed_abstime',
        'diskio_bytesread', 'diskio_byteswritten')]


class ProcessReader:
    def __init__(self, pid):
        self.pid = pid
        self.system = platform.system()
        if self.system == 'Darwin':
            self.lib = ctypes.CDLL('/usr/lib/libproc.dylib', use_errno=True)
            self.lib.proc_pid_rusage.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
            self.lib.proc_pid_rusage.restype = ctypes.c_int
        self.source = 'macOS proc_pid_rusage v2' if self.system == 'Darwin' else 'Linux /proc statm and io'

    def read(self):
        result = {'rss_bytes': None, 'read_bytes': None, 'write_bytes': None, 'errors': []}
        if self.system == 'Darwin':
            info = RusageV2()
            if self.lib.proc_pid_rusage(self.pid, 2, ctypes.byref(info)) == 0:
                result.update(rss_bytes=info.resident_size, read_bytes=info.diskio_bytesread,
                              write_bytes=info.diskio_byteswritten)
            else:
                result['errors'].append(os.strerror(ctypes.get_errno()))
        elif self.system == 'Linux':
            try:
                result['rss_bytes'] = int(Path(f'/proc/{self.pid}/statm').read_text().split()[1]) * os.sysconf('SC_PAGE_SIZE')
            except (OSError, ValueError, IndexError) as e:
                result['errors'].append(str(e))
            try:
                io = dict(line.split(':', 1) for line in Path(f'/proc/{self.pid}/io').read_text().splitlines())
                result.update(read_bytes=int(io['read_bytes']), write_bytes=int(io['write_bytes']))
            except (OSError, ValueError, KeyError) as e:
                result['errors'].append(str(e))
        else:
            result['errors'].append('unsupported process resource sampler')
        return result


class ResourceSampler:
    def __init__(self, pid, interval=.05):
        self.reader = ProcessReader(pid)
        self.interval = interval
        self.stop_event = threading.Event()
        self.samples = []
        self.origin = time.monotonic_ns()
        self.thread = None

    def collect(self):
        start = time.monotonic_ns()
        try:
            values = self.reader.read()
        except Exception as e:
            values = {'rss_bytes': None, 'read_bytes': None, 'write_bytes': None, 'errors': [str(e)]}
        end = time.monotonic_ns()
        self.samples.append(dict(elapsed_ns=(start+end)//2-self.origin,
                                 collection_wall_ns=end-start, **values))

    def start(self):
        self.collect()  # Real baseline before releasing the post-gate handshake.
        def loop():
            while not self.stop_event.wait(self.interval):
                self.collect()
        self.thread = threading.Thread(target=loop, daemon=True)
        self.thread.start()

    def finish(self):
        self.stop_event.set()
        if self.thread:
            self.thread.join()
        return {'schema_version': 1, 'source': self.reader.source,
                'requested_interval_ns': int(self.interval*1e9),
                'scope': 'whole diagnostic child, sampled after gates from immediately before go until child exit; includes handshake, trial loop and final metadata/writes; excludes sampler process',
                'timestamp_scope': 'monotonic midpoint of each OS read, relative to sampler start; independent from allocator timeline origin',
                'overhead': 'launcher thread and OS reads add system load; read duration retained per sample; irregular observations, not instantaneous or reconstructed peaks',
                'io_scope': 'OS-accounted process read/write bytes; includes paging/writes where accounted; not application read calls, queue depth, latency or disk pressure',
                'samples': self.samples}
