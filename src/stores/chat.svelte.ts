import type { ChatCancelled } from '../lib/bindings/ChatCancelled';
import type { ChatDelta } from '../lib/bindings/ChatDelta';
import type { ChatDone } from '../lib/bindings/ChatDone';
import type { ChatErrorEvent } from '../lib/bindings/ChatErrorEvent';
import type { Conversation } from '../lib/bindings/Conversation';
import type { ErrorPayload } from '../lib/bindings/ErrorPayload';
import type { Message } from '../lib/bindings/Message';
import { normalizeError } from '../lib/errors';
import { api } from '../lib/tauri';
import { app } from './app.svelte';

class ChatStore {
  current = $state<Conversation | null>(null);
  sending = $state(false);
  waitingForContinuation = $state(false);
  requestId = $state<string | null>(null);
  pendingAssistantId = $state<string | null>(null);
  error = $state<ErrorPayload | null>(null);
  lastStats = $state<{ tokensPerSec: number; elapsedMs: number } | null>(null);
  private buffer = '';
  private reasoningBuffer = '';

  get messages(): Message[] {
    return this.current?.messages ?? [];
  }

  get lastAssistant(): Message | null {
    const messages = this.messages;
    for (let index = messages.length - 1; index >= 0; index -= 1) {
      if (messages[index].role === 'assistant') return messages[index];
    }
    return null;
  }

  async newChat(): Promise<void> {
    this.current = null;
    this.error = null;
    this.requestId = null;
    this.pendingAssistantId = null;
    this.sending = false;
    this.waitingForContinuation = false;
    this.lastStats = null;
    this.buffer = '';
    this.reasoningBuffer = '';
  }

  async open(conversationId: string): Promise<void> {
    this.error = null;
    this.lastStats = null;
    try {
      this.current = await api.getConversation(conversationId);
      this.sending = this.current.messages.some(
        (message) => message.status === 'streaming',
      );
      this.pendingAssistantId =
        [...this.current.messages]
          .reverse()
          .find(
            (message) =>
              message.role === 'assistant' && message.status === 'streaming',
          )?.id ?? null;
    } catch (error) {
      this.error = normalizeError(error);
    }
  }

  async refresh(): Promise<void> {
    if (!this.current) return;
    try {
      this.current = await api.getConversation(this.current.id);
      this.applyBuffers();
    } catch (error) {
      this.error = normalizeError(error);
    }
  }

  async send(content: string): Promise<void> {
    const text = content.trim();
    if (!text || this.sending) return;
    this.error = null;
    this.lastStats = null;
    try {
      if (!this.current) {
        this.current = await api.createConversation(
          app.bootstrap?.active_model_id ?? null,
        );
      }
      const requestId = crypto.randomUUID();
      this.requestId = requestId;
      this.sending = true;
      this.buffer = '';
      this.reasoningBuffer = '';
      const response = await api.sendMessage({
        conversation_id: this.current.id,
        content: text,
        request_id: requestId,
      });
      await this.refresh();
      this.pendingAssistantId = response.assistant_message_id;
      this.applyBuffers();
      await app.refreshConversations();
    } catch (error) {
      this.sending = false;
      this.requestId = null;
      this.error = normalizeError(error);
    }
  }

  async regenerate(): Promise<void> {
    if (!this.current || this.sending) return;
    this.error = null;
    this.lastStats = null;
    try {
      const requestId = crypto.randomUUID();
      this.requestId = requestId;
      this.sending = true;
      this.buffer = '';
      this.reasoningBuffer = '';
      const response = await api.regenerate(this.current.id, requestId);
      await this.refresh();
      this.pendingAssistantId = response.assistant_message_id;
      this.applyBuffers();
      await app.refreshConversations();
    } catch (error) {
      this.sending = false;
      this.requestId = null;
      this.error = normalizeError(error);
    }
  }

  async stop(): Promise<void> {
    if (!this.requestId) return;
    try {
      await api.cancelGeneration(this.requestId);
    } catch (error) {
      this.error = normalizeError(error);
    }
  }

  async continueResponse(): Promise<void> {
    const message = this.lastAssistant;
    if (
      !this.current ||
      this.sending ||
      !message ||
      !message.content.trim() ||
      !['complete', 'truncated', 'interrupted'].includes(message.status)
    )
      return;
    const previousStatus = message.status;
    this.error = null;
    this.lastStats = null;
    this.buffer = '';
    this.reasoningBuffer = '';
    const requestId = crypto.randomUUID();
    this.requestId = requestId;
    this.pendingAssistantId = message.id;
    this.sending = true;
    this.waitingForContinuation = true;
    message.status = 'streaming';
    try {
      await api.continueResponse(this.current.id, requestId);
    } catch (error) {
      message.status = previousStatus;
      this.waitingForContinuation = false;
      this.sending = false;
      this.requestId = null;
      this.pendingAssistantId = null;
      this.error = normalizeError(error);
    }
  }

  handleDelta(event: ChatDelta): void {
    if (!this.requestId || event.request_id !== this.requestId) return;
    if (event.delta) this.waitingForContinuation = false;
    const message = this.pendingAssistant();
    if (!message) {
      this.buffer += event.delta;
      if (event.reasoning_delta) this.reasoningBuffer += event.reasoning_delta;
      return;
    }
    message.content += event.delta;
    if (event.reasoning_delta) {
      message.reasoning = (message.reasoning ?? '') + event.reasoning_delta;
    }
  }

  async handleDone(event: ChatDone): Promise<void> {
    if (!this.requestId || event.request_id !== this.requestId) return;
    this.waitingForContinuation = false;
    this.sending = false;
    this.lastStats = {
      tokensPerSec: event.tokens_per_sec,
      elapsedMs: Number(event.elapsed_ms),
    };
    this.requestId = null;
    this.pendingAssistantId = null;
    await this.refresh();
    await app.refreshConversations();
  }

  async handleCancelled(event: ChatCancelled): Promise<void> {
    if (!this.requestId || event.request_id !== this.requestId) return;
    this.waitingForContinuation = false;
    this.sending = false;
    this.requestId = null;
    this.pendingAssistantId = null;
    await this.refresh();
  }

  async handleError(event: ChatErrorEvent): Promise<void> {
    if (!this.requestId || event.request_id !== this.requestId) return;
    this.waitingForContinuation = false;
    this.sending = false;
    this.requestId = null;
    this.pendingAssistantId = null;
    this.error = event.error;
    await this.refresh();
  }

  async rename(id: string, title: string): Promise<void> {
    await api.renameConversation(id, title);
    if (this.current?.id === id) this.current.title = title;
    await app.refreshConversations();
  }

  async remove(id: string): Promise<void> {
    await api.deleteConversation(id);
    if (this.current?.id === id) await this.newChat();
    await app.refreshConversations();
  }

  async exportConversation(
    format: 'json' | 'markdown',
  ): Promise<string | null> {
    if (!this.current) return null;
    return api.exportConversation(this.current.id, format);
  }

  private pendingAssistant(): Message | null {
    if (!this.pendingAssistantId) return null;
    return (
      this.messages.find((message) => message.id === this.pendingAssistantId) ??
      null
    );
  }

  private applyBuffers(): void {
    const message = this.pendingAssistant();
    if (message) {
      if (this.buffer) message.content += this.buffer;
      if (this.reasoningBuffer) {
        message.reasoning = (message.reasoning ?? '') + this.reasoningBuffer;
      }
      this.buffer = '';
      this.reasoningBuffer = '';
    }
  }
}

export const chat = new ChatStore();
