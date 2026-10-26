import { marked } from 'marked';
import DOMPurify from 'dompurify';

marked.setOptions({
  gfm: true,
  breaks: true,
});

/**
 * Render assistant Markdown to sanitized HTML. Raw HTML from the model is not
 * trusted: DOMPurify runs with a conservative profile, and inline styles and
 * active content are forbidden.
 */
export function renderMarkdown(text: string): string {
  if (!text) return '';
  const raw = marked.parse(text, { async: false }) as string;
  return DOMPurify.sanitize(raw, {
    USE_PROFILES: { html: true },
    FORBID_TAGS: ['style', 'iframe', 'object', 'embed', 'form', 'input', 'button'],
    FORBID_ATTR: ['style', 'onerror', 'onclick'],
    ALLOW_DATA_ATTR: false,
  });
}
