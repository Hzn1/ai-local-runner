let activeLocale = 'en';

export function setFormatLocale(locale: string): void {
  activeLocale = locale;
}

export function formatNumber(value: number, digits = 0): string {
  return new Intl.NumberFormat(activeLocale, {
    maximumFractionDigits: digits,
    minimumFractionDigits: 0,
  }).format(value);
}

export function formatBytes(value: number | bigint | null | undefined): string {
  const bytes = Number(value ?? 0);
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 MB';
  const gib = bytes / 1024 ** 3;
  if (gib >= 1) return `${formatNumber(gib, 1)} GB`;
  const mib = bytes / 1024 ** 2;
  return `${formatNumber(mib, 0)} MB`;
}

export function formatSpeed(bytesPerSecond: number | bigint): string {
  const speed = Number(bytesPerSecond);
  if (!Number.isFinite(speed) || speed <= 0) return '—';
  if (speed >= 1024 ** 2) return `${formatNumber(speed / 1024 ** 2, 1)} MB/s`;
  return `${formatNumber(speed / 1024, 0)} KB/s`;
}

export function formatDuration(seconds: number | bigint | null): string {
  if (seconds === null) return '—';
  const total = Math.max(0, Math.round(Number(seconds)));
  if (total < 60) return `${total}s`;
  const minutes = Math.floor(total / 60);
  const rest = total % 60;
  if (minutes < 60) return `${minutes}m ${rest}s`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ${minutes % 60}m`;
}

export function formatDate(timestamp: number | bigint | null): string {
  if (timestamp === null) return '—';
  const date = new Date(Number(timestamp));
  if (Number.isNaN(date.getTime())) return '—';
  return new Intl.DateTimeFormat(activeLocale, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(date);
}

export function relativeTime(timestamp: number | bigint | null): string {
  if (timestamp === null) return '—';
  const ms = Number(timestamp);
  if (!Number.isFinite(ms) || ms <= 0) return '—';
  const diffSeconds = Math.round((ms - Date.now()) / 1000);
  const formatter = new Intl.RelativeTimeFormat(activeLocale, {
    numeric: 'auto',
  });
  const units: Array<[Intl.RelativeTimeFormatUnit, number]> = [
    ['year', 365 * 24 * 3600],
    ['month', 30 * 24 * 3600],
    ['day', 24 * 3600],
    ['hour', 3600],
    ['minute', 60],
  ];
  for (const [unit, size] of units) {
    if (Math.abs(diffSeconds) >= size) {
      return formatter.format(Math.round(diffSeconds / size), unit);
    }
  }
  return formatter.format(diffSeconds, 'second');
}

/** Tokens are roughly 3/4 of an English word. Used for honest, plain speed. */
export function tokensToWordsPerSecond(tokensPerSecond: number): number {
  return tokensPerSecond * 0.75;
}

export function percent(done: number | bigint, total: number | bigint): number {
  const totalNumber = Number(total);
  if (!totalNumber || totalNumber <= 0) return 0;
  return Math.min(100, Math.max(0, (Number(done) / totalNumber) * 100));
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
