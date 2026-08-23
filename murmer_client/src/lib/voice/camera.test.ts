import { describe, expect, it } from 'vitest';
import {
  CAMERA_FRAME_RATE,
  CAMERA_PRESETS,
  DEFAULT_CAMERA_QUALITY,
  cameraConstraints,
  cameraPreset,
  isCameraQuality
} from './camera';

/**
 * The constraints only misbehave on hardware that is not to hand: a camera
 * that cannot do the requested resolution, or one that was unplugged since the
 * device id was stored. Both are decided here rather than at the call site, so
 * this is where the `ideal`/`exact` split is worth pinning — getting it the
 * wrong way round fails silently on the developer's machine and fails the
 * call on everybody else's.
 */
describe('camera constraints', () => {
  it('asks for the resolution as a preference, not a requirement', () => {
    const constraints = cameraConstraints('720p');
    // `exact` here would make a camera that cannot do 720p refuse to open
    // rather than hand back the closest thing it has.
    expect(constraints.width).toEqual({ ideal: CAMERA_PRESETS['720p'].width });
    expect(constraints.height).toEqual({ ideal: CAMERA_PRESETS['720p'].height });
    expect(constraints.frameRate).toEqual({ ideal: CAMERA_FRAME_RATE });
  });

  it('pins the device exactly, because "some other camera" is never the ask', () => {
    expect(cameraConstraints('480p', 'cam-1').deviceId).toEqual({ exact: 'cam-1' });
  });

  it('omits the device entirely when none is configured', () => {
    expect(cameraConstraints('480p').deviceId).toBeUndefined();
    expect(cameraConstraints('480p', null).deviceId).toBeUndefined();
    // An empty stored id is "system default", not a device named "".
    expect(cameraConstraints('480p', '').deviceId).toBeUndefined();
  });

  it('falls back to the default preset for anything not in the table', () => {
    // localStorage is user-writable, so a stored quality is untrusted input.
    expect(cameraPreset('1080p')).toEqual(CAMERA_PRESETS[DEFAULT_CAMERA_QUALITY]);
    expect(cameraPreset(null)).toEqual(CAMERA_PRESETS[DEFAULT_CAMERA_QUALITY]);
    expect(cameraPreset(720)).toEqual(CAMERA_PRESETS[DEFAULT_CAMERA_QUALITY]);
    expect(isCameraQuality('1080p')).toBe(false);
    expect(isCameraQuality('720p')).toBe(true);
  });

  it('caps every preset below the screen-share bitrates', () => {
    // Video is a full mesh: one encode per peer. A preset that crept up to
    // screen-share bitrates would cost a channel of five 4x that each way.
    for (const preset of Object.values(CAMERA_PRESETS)) {
      expect(preset.maxBitrate).toBeLessThanOrEqual(1_500_000);
    }
  });
});
