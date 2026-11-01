/** Experimental native API. Only explicit fixture-host open accesses playback. */
export function engineInfo(): {
  experimental: true;
  nativeOutputAvailable: false;
  experimentalFixtureHostAvailable: boolean;
  status: string;
  sampleRate: 48000;
  assetFormat: string;
  deviceIds: string;
  /** Exact Rust offline Limits keys (snake_case); ceilings, not realtime budgets. */
  limits: Record<'assets' | 'asset_frames' | 'nodes' | 'players' | 'outputs' | 'edges' |
    'pending_commands' | 'batch_commands' | 'commands_per_frame' | 'block_frames', number>;
};
export interface OutputConfiguration {
  channels: number;
  minSampleRate: number;
  maxSampleRate: number;
  sampleFormat: string;
}
/** Top-level host/enumeration errors throw. Per-device errors remain visible. */
export function listOutputDevices(): {
  host: string;
  idsEphemeral: true;
  devices: Array<{
    id: string;
    index: number;
    name: string | null;
    nameError: string | null;
    configurations: OutputConfiguration[];
    configurationError: string | null;
  }>;
};
/** Fresh deterministic graph on every call; frames defaults to 10, integer 0..4096. */
export function renderFixture(options?: { frames?: number }): {
  sampleRate: 48000;
  frames: number;
  channels: 3;
  layout: string;
  outputNames: ['fixture.left', 'fixture.right', 'fixture.sum'];
  /** Planar ordinary JS number arrays, converted from native f32; not interleaved. */
  samples: number[][];
  observation: {
    output_frame: number;
    revision: number;
    pending_batches: number;
    players: Array<{
      position: number;
      status: 'Stopped' | 'Playing' | 'Ended';
      loop_region: [number, number] | null;
    }>;
  };
};
