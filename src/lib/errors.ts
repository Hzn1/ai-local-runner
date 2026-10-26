import type { ErrorPayload, RecoveryAction } from './bindings/index';

const RECOVERY_ACTIONS: RecoveryAction[] = [
  'retry',
  'restart_engine',
  'restart_app',
  'free_memory',
  'choose_smaller_model',
  'redownload',
  'import_file',
  'free_disk_space',
  'export_diagnostics',
  'new_chat',
  'reduce_context',
  'compatibility_mode',
  'open_documentation',
];

export function isRecoveryAction(value: string): value is RecoveryAction {
  return (RECOVERY_ACTIONS as string[]).includes(value);
}

/** Convert anything thrown by invoke into a renderable ErrorPayload. */
export function normalizeError(error: unknown): ErrorPayload {
  if (
    typeof error === 'object' &&
    error !== null &&
    'code' in error &&
    'message' in error
  ) {
    const payload = error as Partial<ErrorPayload>;
    return {
      code: String(payload.code ?? 'unknown'),
      message: String(payload.message ?? error),
      detail: payload.detail ?? null,
      recovery: Array.isArray(payload.recovery) ? payload.recovery : [],
      retryable: Boolean(payload.retryable),
    };
  }
  if (error instanceof Error) {
    return {
      code: 'unknown',
      message: error.message,
      detail: null,
      recovery: ['export_diagnostics'],
      retryable: false,
    };
  }
  return {
    code: 'unknown',
    message: String(error),
    detail: null,
    recovery: [],
    retryable: false,
  };
}
