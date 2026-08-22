/**
 * Line diff for the wiki revision history. Kept pure and dependency-free —
 * no stores, no DOM — so the part that is easy to get subtly wrong is unit
 * tested rather than eyeballed in the UI.
 */

/** What happened to one line between the two versions. */
export type DiffOp = 'equal' | 'add' | 'remove';

export type DiffLine = {
  op: DiffOp;
  /** 1-based line number in the old text; null for added lines. */
  oldLine: number | null;
  /** 1-based line number in the new text; null for removed lines. */
  newLine: number | null;
  text: string;
};

/** A run of unchanged lines the view folds away. */
export type DiffGap = { op: 'gap'; count: number };

/** A collapsed diff row: a real line, or a placeholder for hidden ones. */
export type DiffRow = DiffLine | DiffGap;

export type LineDiff = {
  lines: DiffLine[];
  added: number;
  removed: number;
  /**
   * False when the two versions were too large to align line by line and
   * the changed region is reported as one wholesale replacement instead.
   * The result is still correct — just coarser.
   */
  exact: boolean;
};

/**
 * Guard on the alignment matrix. Wiki bodies are capped at 100 kB, which is
 * a few thousand lines; a full O(n·m) table over two such documents is tens
 * of millions of cells. Past this many cells the changed region is reported
 * wholesale instead, which costs detail on exactly the comparisons where
 * a line-level diff would have been unreadable anyway.
 */
const MAX_MATRIX_CELLS = 4_000_000;

/**
 * Split text into lines the way the editor shows them. A single trailing
 * newline is not a final empty line — otherwise every document would diff as
 * having one.
 */
export function splitLines(text: string): string[] {
  if (text === '') return [];
  const lines = text.split(/\r?\n/);
  if (lines.length > 0 && lines[lines.length - 1] === '') lines.pop();
  return lines;
}

function isGap(row: DiffRow): row is DiffGap {
  return row.op === 'gap';
}

/** Longest-common-subsequence table over the already-trimmed middles. */
function lcsLengths(a: string[], b: string[]): Int32Array {
  const width = b.length + 1;
  const table = new Int32Array((a.length + 1) * width);
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      table[i * width + j] =
        a[i] === b[j]
          ? table[(i + 1) * width + j + 1] + 1
          : Math.max(table[(i + 1) * width + j], table[i * width + j + 1]);
    }
  }
  return table;
}

/**
 * Diff two versions of a wiki body line by line, oldest version first.
 *
 * Common leading and trailing lines are trimmed before the alignment runs:
 * a typo fix in a long page then costs almost nothing, and it is what keeps
 * realistic edits well under [`MAX_MATRIX_CELLS`].
 */
export function diffLines(oldText: string, newText: string): LineDiff {
  const a = splitLines(oldText);
  const b = splitLines(newText);
  const lines: DiffLine[] = [];
  let added = 0;
  let removed = 0;
  let exact = true;

  let prefix = 0;
  while (prefix < a.length && prefix < b.length && a[prefix] === b[prefix]) prefix++;
  let suffix = 0;
  while (
    suffix < a.length - prefix &&
    suffix < b.length - prefix &&
    a[a.length - 1 - suffix] === b[b.length - 1 - suffix]
  ) {
    suffix++;
  }

  for (let i = 0; i < prefix; i++) {
    lines.push({ op: 'equal', oldLine: i + 1, newLine: i + 1, text: a[i] });
  }

  const midA = a.slice(prefix, a.length - suffix);
  const midB = b.slice(prefix, b.length - suffix);

  const emitRemoved = (index: number) => {
    removed++;
    lines.push({ op: 'remove', oldLine: prefix + index + 1, newLine: null, text: midA[index] });
  };
  const emitAdded = (index: number) => {
    added++;
    lines.push({ op: 'add', oldLine: null, newLine: prefix + index + 1, text: midB[index] });
  };

  if ((midA.length + 1) * (midB.length + 1) > MAX_MATRIX_CELLS) {
    // Too large to align: report the middle as a wholesale replacement.
    exact = false;
    for (let i = 0; i < midA.length; i++) emitRemoved(i);
    for (let j = 0; j < midB.length; j++) emitAdded(j);
  } else {
    const width = midB.length + 1;
    const table = lcsLengths(midA, midB);
    let i = 0;
    let j = 0;
    while (i < midA.length && j < midB.length) {
      if (midA[i] === midB[j]) {
        lines.push({
          op: 'equal',
          oldLine: prefix + i + 1,
          newLine: prefix + j + 1,
          text: midA[i]
        });
        i++;
        j++;
      } else if (table[(i + 1) * width + j] >= table[i * width + j + 1]) {
        emitRemoved(i);
        i++;
      } else {
        emitAdded(j);
        j++;
      }
    }
    while (i < midA.length) {
      emitRemoved(i);
      i++;
    }
    while (j < midB.length) {
      emitAdded(j);
      j++;
    }
  }

  for (let k = 0; k < suffix; k++) {
    const index = a.length - suffix + k;
    lines.push({
      op: 'equal',
      oldLine: index + 1,
      newLine: b.length - suffix + k + 1,
      text: a[index]
    });
  }

  return { lines, added, removed, exact };
}

/**
 * Fold runs of unchanged lines that are more than `context` lines away from
 * a change into a single gap row, so a one-line fix in a long page does not
 * ask the reader to scroll past the whole document to find it.
 */
export function collapseUnchanged(lines: DiffLine[], context = 3): DiffRow[] {
  const keep = new Array<boolean>(lines.length).fill(false);
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].op === 'equal') continue;
    for (let k = Math.max(0, i - context); k <= Math.min(lines.length - 1, i + context); k++) {
      keep[k] = true;
    }
  }

  const rows: DiffRow[] = [];
  for (let i = 0; i < lines.length; i++) {
    if (keep[i]) {
      rows.push(lines[i]);
      continue;
    }
    const previous = rows[rows.length - 1];
    if (previous && isGap(previous)) {
      previous.count++;
    } else {
      rows.push({ op: 'gap', count: 1 });
    }
  }
  return rows;
}
