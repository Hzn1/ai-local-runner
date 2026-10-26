import { expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { t } from 'svelte-i18n';

vi.mock('./App.svelte', () => ({ default: {} }));
vi.mock('svelte', () => ({ mount: vi.fn() }));

it('initializes translations before mounting the loading screen', async () => {
  const { mount } = await import('svelte');
  const mounted = vi.mocked(mount);
  mounted.mockImplementation(() => {
    // The real App uses these translations during its first render, before
    // onMount or any desktop IPC can run.
    expect(get(t)('app.name')).toBe('TNN Local AI');
    expect(get(t)('app.tagline')).not.toBe('app.tagline');
    return {};
  });
  document.body.innerHTML = '<div id="app"></div>';
  await import('./main');
  await vi.waitFor(() => expect(mounted).toHaveBeenCalledOnce());
});
