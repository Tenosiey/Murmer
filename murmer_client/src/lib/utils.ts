export function normalizeServerUrl(input: string): string {
  let url = input.trim();
  if (!/^wss?:\/\//.test(url)) {
    if (/^https?:\/\//.test(url)) {
      url = url.replace(/^http/, 'ws');
      if (!/\/ws$/.test(url)) {
        url = url.replace(/\/?$/, '/ws');
      }
    } else {
      url = `ws://${url.replace(/\/$/, '')}/ws`;
    }
  }
  return url;
}
