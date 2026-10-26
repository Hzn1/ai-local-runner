import type { BootstrapState } from '../lib/bindings/BootstrapState';
import type { BenchmarkProgress } from '../lib/bindings/BenchmarkProgress';
import type { DownloadProgress } from '../lib/bindings/DownloadProgress';
import type { DownloadStateEvent } from '../lib/bindings/DownloadStateEvent';
import type { EngineStateEvent } from '../lib/bindings/EngineStateEvent';
import type { EngineStatus } from '../lib/bindings/EngineStatus';
import type { ErrorPayload } from '../lib/bindings/ErrorPayload';
import type { ModelCard } from '../lib/bindings/ModelCard';
import type { Settings } from '../lib/bindings/Settings';
import type { UpdateAvailable } from '../lib/bindings/UpdateAvailable';
import type { ConversationSummary } from '../lib/bindings/ConversationSummary';
import { normalizeError } from '../lib/errors';
import { listenEvent } from '../lib/events';
import { setAppLocale, setupI18n, type AppLocale } from '../lib/i18n';
import { api } from '../lib/tauri';

type UnlistenFn = () => void;

class AppStore {
  bootstrap = $state<BootstrapState | null>(null);
  models = $state<ModelCard[]>([]);
  conversations = $state<ConversationSummary[]>([]);
  engine = $state<EngineStatus | null>(null);
  downloads = $state<Record<string, DownloadProgress>>({});
  downloadStates = $state<Record<string, DownloadStateEvent>>({});
  benchmark = $state<BenchmarkProgress | null>(null);
  update = $state<UpdateAvailable | null>(null);
  ready = $state(false);
  fatalError = $state<ErrorPayload | null>(null);
  listeners: UnlistenFn[] = [];
  private systemTheme?: MediaQueryList;
  private initialized = false;

  get settings(): Settings | null {
    return this.bootstrap?.settings ?? null;
  }

  get hardware() {
    return this.bootstrap?.hardware ?? null;
  }

  get activeModel(): ModelCard | null {
    const id = this.bootstrap?.active_model_id;
    if (!id) return null;
    return this.models.find((card) => card.model.id === id) ?? null;
  }

  get installedModels(): ModelCard[] {
    return this.models.filter((card) => card.installed);
  }

  get hasInstalledModel(): boolean {
    return this.models.some((card) => card.installed);
  }

  modelName(modelId: string | null | undefined): string {
    if (!modelId) return '—';
    return (
      this.models.find((card) => card.model.id === modelId)?.model.display_name ??
      this.bootstrap?.installed.find((model) => model.id === modelId)?.display_name ??
      modelId
    );
  }

  async init(): Promise<void> {
    if (this.initialized) return;
    this.initialized = true;
    await setupI18n('en');
    try {
      const [state, models] = await Promise.all([
        api.bootstrap(),
        api.listModels(),
      ]);
      this.bootstrap = state;
      this.models = models;
      this.conversations = state.conversations;
      this.engine = state.engine;
      await setAppLocale(state.settings.language as AppLocale);
      this.watchSystemTheme();
      this.applyAppearance();
    } catch (error) {
      this.fatalError = normalizeError(error);
    } finally {
      this.ready = true;
    }
  }

  async attachEvents(): Promise<() => void> {
    const handlers = await Promise.all([
      listenEvent<EngineStateEvent>('engine://state', (event) => {
        this.handleEngineState(event);
      }),
      listenEvent<DownloadProgress>('download://progress', (event) => {
        this.downloads[event.model_id] = event;
      }),
      listenEvent<DownloadStateEvent>('download://state', (event) => {
        this.handleDownloadState(event);
      }),
      listenEvent<BenchmarkProgress>('benchmark://progress', (event) => {
        this.benchmark = event;
      }),
      listenEvent<UpdateAvailable>('update://available', (event) => {
        this.update = event;
      }),
    ]);
    this.listeners = handlers;
    return () => {
      for (const unlisten of handlers) unlisten();
      this.listeners = [];
    };
  }

  private handleEngineState(event: EngineStateEvent): void {
    this.engine = {
      state: event.state,
      model_id: event.model_id,
      tier: event.tier,
      compatibility_mode: event.compatibility_mode,
      uptime_secs: this.engine?.uptime_secs ?? null,
      idle_secs: 0n,
    };
    if (event.error) {
      // Chat views surface their own errors; keep this for the footer badge.
      console.warn('engine error', event.error);
    }
  }

  private handleDownloadState(event: DownloadStateEvent): void {
    this.downloadStates[event.model_id] = event;
    if (event.state === 'installed') {
      void this.refreshAfterInstall();
    }
  }

  async refreshAfterInstall(): Promise<void> {
    try {
      const [models, state] = await Promise.all([
        api.listModels(),
        api.bootstrap(),
      ]);
      this.models = models;
      this.bootstrap = state;
      this.conversations = state.conversations;
      this.engine = state.engine;
      this.applyAppearance();
    } catch (error) {
      console.warn('could not refresh after install', normalizeError(error));
    }
  }

  async refreshModels(): Promise<void> {
    this.models = await api.listModels();
  }

  async refreshConversations(): Promise<void> {
    this.conversations = await api.listConversations();
  }

  async refreshEngine(): Promise<void> {
    this.engine = await api.engineStatus();
  }

  async updateSettings(patch: Record<string, unknown>): Promise<Settings> {
    const settings = await api.updateSettings(patch);
    if (this.bootstrap) {
      this.bootstrap.settings = settings;
      if (settings.active_model_id !== undefined) {
        this.bootstrap.active_model_id = settings.active_model_id;
      }
    }
    if (patch.language) {
      await setAppLocale(settings.language as AppLocale);
    }
    this.applyAppearance();
    return settings;
  }

  async setLanguage(language: AppLocale): Promise<void> {
    await this.updateSettings({ language });
  }

  async setActiveModel(modelId: string): Promise<void> {
    await this.updateSettings({ active_model_id: modelId });
    if (this.bootstrap) this.bootstrap.active_model_id = modelId;
  }

  async startEngine(modelId?: string): Promise<void> {
    const target = modelId ?? this.bootstrap?.active_model_id;
    if (!target) return;
    try {
      const info = await api.startEngine(target);
      this.engine = {
        state: 'ready',
        model_id: info.model_id,
        tier: info.tier,
        compatibility_mode: info.compatibility_mode,
        uptime_secs: 0n,
        idle_secs: 0n,
      };
    } catch (error) {
      this.engine = {
        state: 'failed',
        model_id: target,
        tier: null,
        compatibility_mode: false,
        uptime_secs: null,
        idle_secs: null,
      };
      throw error;
    }
  }

  async stopEngine(): Promise<void> {
    await api.stopEngine();
    this.engine = {
      state: 'stopped',
      model_id: null,
      tier: null,
      compatibility_mode: false,
      uptime_secs: null,
      idle_secs: null,
    };
  }

  downloadFor(modelId: string): DownloadProgress | null {
    return this.downloads[modelId] ?? null;
  }

  downloadStateFor(modelId: string): string {
    return this.downloadStates[modelId]?.state ?? 'idle';
  }

  applyAppearance(): void {
    const settings = this.settings;
    if (!settings) return;
    const prefersDark = this.systemTheme?.matches ?? false;
    const dark =
      settings.theme === 'dark' ||
      (settings.theme === 'system' && prefersDark);
    document.documentElement.classList.toggle('dark', dark);
    document.documentElement.style.setProperty(
      '--font-scale',
      String(settings.font_scale ?? 1),
    );
    document.documentElement.dataset.theme = settings.theme;
  }

  private watchSystemTheme(): void {
    if (typeof window === 'undefined' || !window.matchMedia) return;
    this.systemTheme = window.matchMedia('(prefers-color-scheme: dark)');
    this.systemTheme.addEventListener('change', () => {
      if (this.settings?.theme === 'system') this.applyAppearance();
    });
  }
}

export const app = new AppStore();
