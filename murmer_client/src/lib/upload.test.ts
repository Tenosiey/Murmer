import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  loadFile,
  setUploadSession,
  uploadAttachment,
  uploadForm,
  uploadErrorMessage
} from './upload';

describe('uploadForm', () => {
  afterEach(() => setUploadSession(null));

  it('sends the connection\'s upload session ahead of the file', () => {
    setUploadSession('challenge-1');
    const form = uploadForm(new Blob(['data']), 'cat.png');

    expect(form.get('session')).toBe('challenge-1');
    // The server authenticates as soon as it reaches the file part, so a file
    // sent first would be rejected however valid the session behind it.
    expect([...form.keys()]).toEqual(['session', 'file']);
  });

  it('keeps the filename the server classifies the upload by', () => {
    const named = uploadForm(new Blob(['data']), 'cat.png').get('file') as File;
    expect(named.name).toBe('cat.png');

    // A File already carries its name; passing it through unchanged is what
    // keeps the extension safe-list looking at the real one.
    const file = new File(['data'], 'notes.pdf');
    expect((uploadForm(file).get('file') as File).name).toBe('notes.pdf');
  });
});

describe('uploadErrorMessage', () => {
  it('explains the statuses the endpoint answers with', () => {
    expect(uploadErrorMessage(401)).toMatch(/rejected/i);
    expect(uploadErrorMessage(403)).toMatch(/rejected/i);
    expect(uploadErrorMessage(429)).toMatch(/too quickly/i);
    expect(uploadErrorMessage(413, 'image')).toBe('That image is too large to upload.');
    expect(uploadErrorMessage(415, 'sound')).toBe('This sound type is not allowed on the server.');
    expect(uploadErrorMessage(507)).toMatch(/upload space/i);
  });

  it('leaves other statuses to the caller', () => {
    expect(uploadErrorMessage(200)).toBeNull();
    expect(uploadErrorMessage(500)).toBeNull();
  });
});

describe('uploadAttachment', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  function answer(status: number, body: unknown = {}) {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response(JSON.stringify(body), { status }))
    );
  }

  it('resolves the stored path against the server and keeps images inline', async () => {
    answer(200, { url: '/files/abc.png', kind: 'image' });
    const result = await uploadAttachment('http://srv', new File(['x'], 'a.png'), 1024);
    expect(result).toEqual({ ok: true, content: { image: 'http://srv/files/abc.png' } });
  });

  it('names an attachment after what the server stored', async () => {
    answer(200, { url: '/files/k', name: 'report.pdf', size: 3 });
    const result = await uploadAttachment('http://srv', new File(['x'], 'local.pdf'), 1024);
    expect(result).toEqual({
      ok: true,
      content: { attachment: { url: 'http://srv/files/k', name: 'report.pdf', size: 3 } }
    });
  });

  it('names the current limit when the server refuses the size', async () => {
    answer(413);
    const result = await uploadAttachment('http://srv', new File(['x'], 'a.bin'), 8 * 1024 * 1024);
    expect(result).toEqual({ ok: false, message: 'File is too large to upload (limit: 8 MB).' });
  });

  it('fails rather than posting a message with no URL in it', async () => {
    // Otherwise the chat would carry a link to "http://srvundefined".
    vi.spyOn(console, 'error').mockImplementation(() => {});
    answer(200, { kind: 'image' });
    const result = await uploadAttachment('http://srv', new File(['x'], 'a.png'), 1024);
    expect(result).toEqual({ ok: false, message: 'File upload failed.' });
  });
});

describe('sealed attachments', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('uploads only ciphertext and opens it again with the key it returns', async () => {
    const plain = new TextEncoder().encode('Medical report for Jane Doe');
    let posted: FormData | undefined;
    vi.stubGlobal(
      'fetch',
      vi.fn(async (_url: string, init?: RequestInit) => {
        posted = init?.body as FormData;
        return new Response(JSON.stringify({ url: '/files/1-abc.bin', kind: 'file' }));
      })
    );

    const file = new File([plain], 'Medical_Report_Jane_Doe.pdf');
    const result = await uploadAttachment('http://srv', file, 1024, true);

    expect([...posted!.keys()]).toEqual(['session', 'sealed', 'file']);
    const stored = posted!.get('file') as File;
    // Only the extension leaves the client; it picks the upload category.
    expect(stored.name).toBe('sealed.pdf');
    const bytes = new Uint8Array(await stored.arrayBuffer());
    expect(new TextDecoder().decode(bytes)).not.toContain('Jane');

    if (!result.ok || !('attachment' in result.content)) throw new Error('no attachment');
    const { attachment } = result.content;
    expect(attachment.name).toBe('Medical_Report_Jane_Doe.pdf');
    expect(attachment.key).toBeTypeOf('string');

    vi.stubGlobal('fetch', vi.fn(async () => new Response(bytes)));
    const opened = await loadFile(attachment.url, attachment.key);
    expect(new Uint8Array(await opened.arrayBuffer())).toEqual(plain);
    await expect(loadFile(attachment.url, btoa('x'.repeat(32)))).rejects.toThrow();
  });
});
