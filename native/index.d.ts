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
export interface FixtureHostOptions {
  deviceId: string;
  expectedName: string;
  channels: number;
  mappings: Array<{ output: 'fixture.left' | 'fixture.right'; physical: number }>;
}
export interface FixtureHostSnapshot {
  closed: boolean;
  queuedBatches: number;
  queueFull: number;
  /** Separate from frame-coherent observation; not an underrun count. */
  streamErrors: number;
  observation: null | {
    outputFrame: number;
    players: Array<{ sourceFrame: number; status: 0 | 1 | 2; loop: [number, number] | null }>;
    callbacks: number;
    appliedBatches: number;
    rejectedBatches: number;
    processedBatches: number;
    /** Renderer adapter timing only; excludes publication/CPAL overhead and DAC timing. */
    processDeadlineMisses: number;
    maxProcessNanos: number;
    publicationDrops: number;
    processErrors: number;
    lastErrorCode: 0 | 1 | 2;
  };
}
export interface QueuedFixtureControl { queuedBatch: number; applied: false }
export class ExperimentalFixtureHost {
  private constructor();
  play(): QueuedFixtureControl;
  pause(): QueuedFixtureControl;
  seek(sourceFrame: number): QueuedFixtureControl;
  setLoop(region: [number, number] | null): QueuedFixtureControl;
  /** Bounded read, may return stale state; null until first callback publication. */
  snapshot(): FixtureHostSnapshot;
  /** Idempotent; Linux worker join may block the calling thread. */
  close(): void;
}
/** EXPLICIT HARDWARE OPT-IN. Opens/starts Linux ALSA output; initially silent.
 * Short RAM sine fixture ONLY, never recording playback. No automated opening tests.
 */
export function openExperimentalFixtureHost(options: FixtureHostOptions): ExperimentalFixtureHost;

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
