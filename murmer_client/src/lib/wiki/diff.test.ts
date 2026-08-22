import { describe, expect, it } from 'vitest';
import { collapseUnchanged, diffLines, splitLines, type DiffLine } from './diff';

/** Compact rendering of a diff: one `+`/`-`/` ` prefixed line per row. */
function render(lines: DiffLine[]): string[] {
  return lines.map(
    (line) => `${line.op === 'add' ? '+' : line.op === 'remove' ? '-' : ' '}${line.text}`
  );
}

describe('splitLines', () => {
  it('treats a single trailing newline as a terminator, not a line', () => {
    expect(splitLines('a\nb\n')).toEqual(['a', 'b']);
    expect(splitLines('a\nb')).toEqual(['a', 'b']);
    expect(splitLines('a\nb\n\n')).toEqual(['a', 'b', '']);
    expect(splitLines('')).toEqual([]);
  });

  it('accepts CRLF, which a paste from another editor carries', () => {
    expect(splitLines('a\r\nb\r\n')).toEqual(['a', 'b']);
  });
});

describe('diffLines', () => {
  it('reports an unchanged document as all equal', () => {
    const diff = diffLines('one\ntwo', 'one\ntwo');
    expect(render(diff.lines)).toEqual([' one', ' two']);
    expect(diff.added).toBe(0);
    expect(diff.removed).toBe(0);
    expect(diff.exact).toBe(true);
  });

  it('pairs a replaced line as a removal followed by an addition', () => {
    const diff = diffLines('intro\nold middle\nend', 'intro\nnew middle\nend');
    expect(render(diff.lines)).toEqual([' intro', '-old middle', '+new middle', ' end']);
    expect(diff.added).toBe(1);
    expect(diff.removed).toBe(1);
  });

  it('keeps the surviving lines instead of replacing the whole block', () => {
    const diff = diffLines('a\nb\nc', 'a\nx\nb\nc');
    expect(render(diff.lines)).toEqual([' a', '+x', ' b', ' c']);
    expect(diff.removed).toBe(0);
  });

  it('numbers lines against their own side of the diff', () => {
    const diff = diffLines('a\nb\nc', 'a\nc');
    const removedLine = diff.lines.find((line) => line.op === 'remove');
    expect(removedLine).toMatchObject({ text: 'b', oldLine: 2, newLine: null });
    const lastLine = diff.lines[diff.lines.length - 1];
    // 'c' moved up a line in the new version but not in the old one.
    expect(lastLine).toMatchObject({ text: 'c', oldLine: 3, newLine: 2 });
  });

  it('handles creation and deletion of the whole body', () => {
    expect(render(diffLines('', 'hello').lines)).toEqual(['+hello']);
    expect(render(diffLines('hello', '').lines)).toEqual(['-hello']);
  });

  it('stays exact for a small edit inside a document of thousands of lines', () => {
    // The prefix/suffix trim is what keeps the alignment matrix small here;
    // without it this pair would trip the guard and diff as one big block.
    const long = Array.from({ length: 5000 }, (_, i) => `line ${i}`);
    const edited = [...long];
    edited[2500] = 'line 2500 (fixed)';
    const diff = diffLines(long.join('\n'), edited.join('\n'));
    expect(diff.exact).toBe(true);
    expect(diff.added).toBe(1);
    expect(diff.removed).toBe(1);
    expect(diff.lines).toHaveLength(5001);
  });

  it('falls back to a wholesale replacement when the versions are too large to align', () => {
    // Two entirely different documents of a few thousand lines each: nothing
    // to trim, so the matrix guard decides.
    const a = Array.from({ length: 3000 }, (_, i) => `old ${i}`).join('\n');
    const b = Array.from({ length: 3000 }, (_, i) => `new ${i}`).join('\n');
    const diff = diffLines(a, b);
    expect(diff.exact).toBe(false);
    expect(diff.removed).toBe(3000);
    expect(diff.added).toBe(3000);
    // Removals first, then the additions — a replacement, not an alignment.
    expect(diff.lines[0].op).toBe('remove');
    expect(diff.lines[3000].op).toBe('add');
  });
});

describe('collapseUnchanged', () => {
  it('folds unchanged runs outside the context window', () => {
    const lines = diffLines(
      Array.from({ length: 20 }, (_, i) => `line ${i}`).join('\n'),
      Array.from({ length: 20 }, (_, i) => (i === 10 ? 'changed' : `line ${i}`)).join('\n')
    ).lines;
    const rows = collapseUnchanged(lines, 2);

    expect(rows[0]).toEqual({ op: 'gap', count: 8 });
    expect(rows.map((row) => (row.op === 'gap' ? `…${row.count}` : row.op))).toEqual([
      '…8',
      'equal',
      'equal',
      'remove',
      'add',
      'equal',
      'equal',
      '…7'
    ]);
  });

  it('folds an unchanged document into a single gap', () => {
    const lines = diffLines('a\nb\nc', 'a\nb\nc').lines;
    expect(collapseUnchanged(lines)).toEqual([{ op: 'gap', count: 3 }]);
  });

  it('keeps everything when nothing is far enough from a change', () => {
    const lines = diffLines('a\nb', 'a\nc').lines;
    expect(collapseUnchanged(lines, 3)).toEqual(lines);
  });
});
