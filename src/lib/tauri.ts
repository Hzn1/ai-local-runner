import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import type { BootstrapState } from './bindings/BootstrapState';
import type { BenchmarkResult } from './bindings/BenchmarkResult';
import type { Conversation } from './bindings/Conversation';
import type { ConversationSummary } from './bindings/ConversationSummary';
import type { EngineInfo } from './bindings/EngineInfo';
import type { EngineStatus } from './bindings/EngineStatus';
import type { HardwareProfile } from './bindings/HardwareProfile';
import type { ModelCard } from './bindings/ModelCard';
import type { SendMessageRequest } from './bindings/SendMessageRequest';
import type { SendMessageResponse } from './bindings/SendMessageResponse';
import type { Settings } from './bindings/Settings';
import type { StorageSummary } from './bindings/StorageSummary';
import type { UpdateAvailable } from './bindings/UpdateAvailable';

/** True when running inside the Tauri webview rather than a plain browser. */
export const isTauri =
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export class NotInDesktopError extends Error {
  constructor() {
    super('This screen needs the desktop app. Run `npm run tauri dev`.');
    this.name = 'NotInDesktopError';
  }
}

async function invoke<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauri) throw new NotInDesktopError();
  return tauriInvoke<T>(command, args);
}

export const api = {
  bootstrap: () => invoke<BootstrapState>('get_bootstrap_state'),
  scanHardware: () => invoke<HardwareProfile>('scan_hardware'),
  listModels: () => invoke<ModelCard[]>('list_models'),

  downloadModel: (modelId: string) =>
    invoke<void>('download_model', { modelId }),
  pauseDownload: (modelId: string) =>
    invoke<void>('pause_download', { modelId }),
  resumeDownload: (modelId: string) =>
    invoke<void>('resume_download', { modelId }),
  cancelDownload: (modelId: string) =>
    invoke<void>('cancel_download', { modelId }),
  deleteModel: (modelId: string) => invoke<void>('delete_model', { modelId }),
  importModel: (path: string) => invoke<ModelCard>('import_model', { path }),
  storageSummary: () => invoke<StorageSummary>('get_storage_summary'),

  startEngine: (modelId: string) =>
    invoke<EngineInfo>('start_engine', { modelId }),
  stopEngine: () => invoke<void>('stop_engine'),
  engineStatus: () => invoke<EngineStatus>('engine_status'),

  sendMessage: (request: SendMessageRequest) =>
    invoke<SendMessageResponse>('send_message', { request }),
  cancelGeneration: (requestId: string) =>
    invoke<void>('cancel_generation', { requestId }),
  continueResponse: (conversationId: string, requestId: string) =>
    invoke<SendMessageResponse>('continue_response', {
      conversationId,
      requestId,
    }),
  regenerate: (conversationId: string, requestId: string) =>
    invoke<SendMessageResponse>('regenerate', { conversationId, requestId }),

  listConversations: () => invoke<ConversationSummary[]>('list_conversations'),
  getConversation: (id: string) =>
    invoke<Conversation>('get_conversation', { id }),
  createConversation: (modelId?: string | null) =>
    invoke<Conversation>('create_conversation', { modelId: modelId ?? null }),
  renameConversation: (id: string, title: string) =>
    invoke<void>('rename_conversation', { id, title }),
  deleteConversation: (id: string) =>
    invoke<void>('delete_conversation', { id }),
  exportConversation: (id: string, format: 'json' | 'markdown') =>
    invoke<string>('export_conversation', { id, format }),

  getSettings: () => invoke<Settings>('get_settings'),
  updateSettings: (patch: Record<string, unknown>) =>
    invoke<Settings>('update_settings', { patch }),

  runBenchmark: (modelId: string) =>
    invoke<BenchmarkResult>('run_benchmark', { modelId }),
  exportDiagnostics: () => invoke<string>('export_diagnostics'),
  openDataDir: () => invoke<void>('open_data_dir'),
  openLogs: () => invoke<void>('open_logs'),
  checkForUpdates: () => invoke<UpdateAvailable | null>('check_for_updates'),
};
