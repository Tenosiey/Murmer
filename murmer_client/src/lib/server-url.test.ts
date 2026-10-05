/**
 * `serverFileUrl` is what keeps a message image from being a tracking pixel:
 * anything it lets through, every reader's client fetches.
 */
import { describe, expect, it } from 'vitest';
import { serverFileUrl } from './server-url';

const base = 'https://chat.example.test';

describe('serverFileUrl', () => {
  it('accepts files on the connected server', () => {
    expect(serverFileUrl(`${base}/files/1-abc-cat.png`, base)).toBe(`${base}/files/1-abc-cat.png`);
    expect(serverFileUrl('/files/1-abc-cat.png', base)).toBe(`${base}/files/1-abc-cat.png`);
  });

  it('refuses anything another host would serve', () => {
    for (const url of [
      'https://evil.test/files/pixel.png',
      'https://chat.example.test.evil.test/files/p.png',
      `${base}:8443/files/p.png`,
      `${base}/avatar.png`,
      `${base}/files/../link-preview?url=x`,
      '//evil.test/files/p.png',
      'data:image/png;base64,AAAA',
      42
    ]) {
      expect(serverFileUrl(url, base), String(url)).toBeNull();
    }
  });

  it('refuses everything before a server is selected', () => {
    expect(serverFileUrl(`${base}/files/a.png`, '')).toBeNull();
  });
});
