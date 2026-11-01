'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const audio = require('..');

test('real addon exports and explicit offline limits', () => {
  assert.deepEqual(Object.keys(audio).sort(), ['ExperimentalFixtureHost', 'engineInfo', 'listOutputDevices', 'openExperimentalFixtureHost', 'renderFixture']);
  assert.equal(typeof audio.openExperimentalFixtureHost, 'function');
  // Never invoke open: even a negative stream-opening test is not authorized.
  const info = audio.engineInfo();
  assert.equal(info.experimental, true);
  assert.equal(info.nativeOutputAvailable, false);
  assert.equal(info.sampleRate, 48000);
  assert.equal(info.limits.outputs, 16);
  assert.equal(info.limits.block_frames, 4096);
});

test('literal planar graph output, alignment, named routing and observations', () => {
  const result = audio.renderFixture();
  assert.deepEqual(result.outputNames, ['fixture.left', 'fixture.right', 'fixture.sum']);
  assert.equal(result.channels, 3);
  assert.equal(result.frames, 10);
  assert.deepEqual(result.samples, [
    [0, .25, -.5, .75, -1, 0, 0, 0, 0, 0],
    [0, 0, .0625, .125, .25, .375, 0, 0, 0, 0],
    [0, .25, -.4375, .875, -.75, .375, 0, 0, 0, 0],
  ]);
  assert.equal(result.observation.output_frame, 10);
  assert.equal(result.observation.pending_batches, 0);
  assert.deepEqual(result.observation.players.map(p => [p.position, p.status]), [[4, 'Ended'], [4, 'Ended']]);
  assert.deepEqual(audio.renderFixture(), result);
  for (const frames of [0, 1, 2, 5, 10, 4096]) {
    const other = audio.renderFixture({ frames });
    assert.equal(other.observation.output_frame, frames);
    for (let c = 0; c < 3; c++) {
      assert.equal(other.samples[c].length, frames);
      assert.deepEqual(other.samples[c].slice(0, 10), result.samples[c].slice(0, frames));
    }
  }
});

test('reject malformed/out of budget requests; subsequent render unchanged', () => {
  for (const options of [{ frames: -1 }, { frames: 4097 }, { frames: 1.5 },
    { frames: '10' }, { frames: null }, { frames: NaN }, { frames: Infinity },
    { unknown: true }, [], 'bad', 7]) {
    assert.throws(() => audio.renderFixture(options));
  }
  assert.equal(audio.renderFixture().frames, 10);
});

test('actual native capability enumeration only (no streams)', () => {
  // A host failure is a real failure, not an invented empty device list.
  const result = audio.listOutputDevices();
  assert.equal(result.idsEphemeral, true);
  assert.equal(typeof result.host, 'string');
  assert.ok(Array.isArray(result.devices));
  for (const device of result.devices) {
    assert.equal(typeof device.id, 'string');
    assert.ok(device.name === null || typeof device.name === 'string');
    assert.ok(device.configurationError === null || typeof device.configurationError === 'string');
    for (const config of device.configurations) {
      assert.ok(config.channels > 0);
      assert.ok(config.minSampleRate > 0);
      assert.ok(config.maxSampleRate >= config.minSampleRate);
      assert.equal(typeof config.sampleFormat, 'string');
    }
  }
  console.log('Native enumeration:', JSON.stringify({ host: result.host,
    devices: result.devices.map(d => ({ name: d.name, configurations: d.configurations.length,
      nameError: d.nameError, configurationError: d.configurationError })) }));
});
