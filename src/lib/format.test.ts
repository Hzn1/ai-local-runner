import { beforeEach, describe, expect, it } from 'vitest';
import {
  clamp,
  formatBytes,
  formatDuration,
  formatSpeed,
  percent,
  setFormatLocale,
  tokensToWordsPerSecond,
} from './format';

beforeEach(() => setFormatLocale('en'));

describe('formatBytes', () => {
  it('uses MB below one GiB and GB above', () => {
    expect(formatBytes(0)).toBe('0 MB');
    expect(formatBytes(null)).toBe('0 MB');
    expect(formatBytes(512 * 1024 * 1024)).toBe('512 MB');
    expect(formatBytes(2.5 * 1024 ** 3)).toBe('2.5 GB');
  });

  it('accepts bigint from the IPC layer', () => {
    expect(formatBytes(517_312_992n)).toBe('493 MB');
  });
});

describe('formatSpeed', () => {
  it('switches between KB/s and MB/s', () => {
    expect(formatSpeed(0)).toBe('—');
    expect(formatSpeed(512 * 1024)).toBe('512 KB/s');
    expect(formatSpeed(1.5 * 1024 ** 2)).toBe('1.5 MB/s');
  });
});

describe('formatDuration', () => {
  it('formats seconds, minutes and hours', () => {
    expect(formatDuration(null)).toBe('—');
    expect(formatDuration(9)).toBe('9s');
    expect(formatDuration(65)).toBe('1m 5s');
    expect(formatDuration(3 * 3600 + 61)).toBe('3h 1m');
  });
});

describe('progress helpers', () => {
  it('clamps percentages and handles unknown totals', () => {
    expect(percent(0, 100)).toBe(0);
    expect(percent(50, 100)).toBe(50);
    expect(percent(150, 100)).toBe(100);
    expect(percent(10, 0)).toBe(0);
    expect(percent(1n, 4n)).toBe(25);
  });

  it('clamps numbers', () => {
    expect(clamp(5, 0, 10)).toBe(5);
    expect(clamp(-1, 0, 10)).toBe(0);
    expect(clamp(11, 0, 10)).toBe(10);
  });

  it('converts tokens to words for plain-language speed', () => {
    expect(tokensToWordsPerSecond(10)).toBe(7.5);
  });
});
