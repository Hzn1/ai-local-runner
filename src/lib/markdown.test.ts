import { describe, expect, it } from 'vitest';
import { renderMarkdown } from './markdown';

describe('renderMarkdown', () => {
  it('renders headings, emphasis and lists', () => {
    const html = renderMarkdown('# Title\n\nSome **bold** text.\n\n- one\n- two');
    expect(html).toContain('<h1');
    expect(html).toContain('<strong>bold</strong>');
    expect(html).toContain('<li>one</li>');
  });

  it('returns an empty string for empty input', () => {
    expect(renderMarkdown('')).toBe('');
  });

  it('strips scripts, styles and event handlers', () => {
    const html = renderMarkdown(
      '<script>alert(1)</script><style>body{display:none}</style>' +
        '<img src="x" onerror="alert(1)"><p onclick="alert(1)">click</p>',
    );
    expect(html).not.toContain('<script');
    expect(html).not.toContain('<style');
    expect(html).not.toContain('onerror');
    expect(html).not.toContain('onclick');
  });

  it('removes javascript: links but keeps safe links', () => {
    const unsafe = renderMarkdown('[go](javascript:alert(1))');
    expect(unsafe).not.toContain('javascript:');

    const safe = renderMarkdown('[docs](https://example.com/help)');
    expect(safe).toContain('href="https://example.com/help"');
  });

  it('forbids forms and embedded content', () => {
    const html = renderMarkdown(
      '<iframe src="https://example.com"></iframe><form><input value="x"></form>',
    );
    expect(html).not.toContain('<iframe');
    expect(html).not.toContain('<form');
    expect(html).not.toContain('<input');
  });
});
