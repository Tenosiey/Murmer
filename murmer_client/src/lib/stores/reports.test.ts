import { describe, expect, it } from 'vitest';
import { parseReport } from './reports';

describe('parseReport', () => {
  it('keeps a well-formed row', () => {
    const row = {
      id: 1,
      messageId: 9,
      channelId: 2,
      author: 'carol',
      reporter: 'bob',
      reason: 'spam',
      createdAt: '2026-10-11T12:00:00Z',
      deleted: false,
      excerpt: 'buy now'
    };
    expect(parseReport(row)).toEqual(row);
  });

  it('drops a row without its ids and defaults the rest', () => {
    expect(parseReport({ id: 1, channelId: 2 })).toBeNull();
    expect(parseReport(null)).toBeNull();
    expect(parseReport({ id: 1, messageId: 9, channelId: 2, excerpt: null })).toEqual({
      id: 1,
      messageId: 9,
      channelId: 2,
      author: '',
      reporter: '',
      reason: '',
      createdAt: null,
      deleted: false,
      excerpt: null
    });
  });
});
