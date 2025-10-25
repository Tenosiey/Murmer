export function extractLinks(text: string | undefined | null): string[] {
  if (!text) return [];
  const urlPattern = /https?:\/\/[\w.-]+(?:\/[\w\-./?%&=+#@~:,;!]*)?/gi;
  const results = new Set<string>();
  let match: RegExpExecArray | null;
  while ((match = urlPattern.exec(text)) !== null) {
    let url = match[0];
    // Trim common trailing punctuation
    url = url.replace(/[).,!?"'\]]+$/g, '');
    try {
      const parsed = new URL(url);
      if (parsed.protocol === 'http:' || parsed.protocol === 'https:') {
        results.add(parsed.toString());
      }
    } catch (error) {
      // ignore invalid URLs
    }
  }
  return [...results];
}
