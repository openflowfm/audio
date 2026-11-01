import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
const root = fileURLToPath(new URL('../', import.meta.url));
const target = path.join(root, 'target');
const build = spawnSync('cargo', ['build', '--locked', '--release', '--target-dir', target], {
  cwd: root, stdio: 'inherit',
});
if (build.error) throw build.error;
if (build.status !== 0) process.exit(build.status ?? 1);
const filename = process.platform === 'win32' ? 'audio_flow_native_experimental.dll'
  : process.platform === 'darwin' ? 'libaudio_flow_native_experimental.dylib'
  : 'libaudio_flow_native_experimental.so';
mkdirSync(path.join(root, 'dist'), { recursive: true });
copyFileSync(path.join(target, 'release', filename), path.join(root, 'dist', 'audio-flow.node'));
