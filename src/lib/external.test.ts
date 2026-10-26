import { afterEach, describe, expect, it, vi } from 'vitest';
import { openExternal, safeExternalUrl } from './external';

afterEach(() => vi.restoreAllMocks());

describe('safeExternalUrl', () => {
  it('accepts http and https', () => {
    expect(safeExternalUrl('https://huggingface.co/model')).toBe(
      'https://huggingface.co/model',
    );
    expect(safeExternalUrl('  http://example.com/a  ')).toBe(
      'http://example.com/a',
    );
  });

  it('rejects active and local schemes', () => {
    expect(safeExternalUrl('javascript:alert(1)')).toBeNull();
    expect(safeExternalUrl('file:///etc/passwd')).toBeNull();
    expect(safeExternalUrl('data:text/html,<h1>x</h1>')).toBeNull();
    expect(safeExternalUrl('mailto:someone@example.com')).toBeNull();
    expect(safeExternalUrl('not a url')).toBeNull();
    expect(safeExternalUrl('')).toBeNull();
    expect(safeExternalUrl('   ')).toBeNull();
  });
});

describe('openExternal', () => {
  it('opens a safe URL in the browser', async () => {
    const open = vi.spyOn(window, 'open').mockReturnValue(null);
    await expect(openExternal('https://example.com')).resolves.toBe(true);
    expect(open).toHaveBeenCalledWith(
      'https://example.com/',
      '_blank',
      'noopener,noreferrer',
    );
  });

  it('refuses to open unsafe URLs', async () => {
    const open = vi.spyOn(window, 'open').mockReturnValue(null);
    await expect(openExternal('javascript:alert(1)')).resolves.toBe(false);
    expect(open).not.toHaveBeenCalled();
  });
});
