import { beforeEach, expect, it, vi } from 'vitest';
import type { Conversation } from '../lib/bindings/Conversation';
import { api } from '../lib/tauri';
import { chat } from './chat.svelte';

vi.mock('../lib/tauri', () => ({
  api: {
    continueResponse: vi.fn(),
    getConversation: vi.fn(),
  },
}));
vi.mock('./app.svelte', () => ({ app: { refreshConversations: vi.fn() } }));

function conversation(): Conversation {
  return {
    id: 'conversation',
    title: 'Test',
    model_id: 'model',
    created_at: 0n,
    updated_at: 0n,
    pinned: false,
    messages: [
      {
        id: 'assistant',
        conversation_id: 'conversation',
        role: 'assistant',
        content: 'The first half',
        reasoning: null,
        status: 'truncated',
        token_count: 256,
        created_at: 0n,
        seq: 1n,
      },
    ],
  };
}

beforeEach(async () => {
  vi.resetAllMocks();
  await chat.newChat();
  chat.current = conversation();
});

it.each(['truncated', 'complete', 'interrupted'] as const)(
  'continues a %s answer and keeps the prefix while streaming and after reload',
  async (status) => {
    chat.current!.messages[0].status = status;
    vi.mocked(api.continueResponse).mockImplementation(
      async (_id, requestId) => {
        chat.handleDelta({
          request_id: requestId,
          delta: ' and the ending.',
          reasoning_delta: null,
          index: 0,
        });
        return {
          request_id: requestId,
          user_message_id: 'user',
          assistant_message_id: 'assistant',
        };
      },
    );
    await chat.continueResponse();
    expect(chat.messages).toHaveLength(1);
    expect(chat.messages[0].content).toBe('The first half and the ending.');
    const stored = conversation();
    stored.messages[0].content += ' and the ending.';
    stored.messages[0].status = 'complete';
    vi.mocked(api.getConversation).mockResolvedValue(stored);
    await chat.handleDone({
      request_id: chat.requestId!,
      usage: { prompt_tokens: 10, completion_tokens: 10, total_tokens: 20 },
      elapsed_ms: 1000n,
      tokens_per_sec: 10,
    });
    expect(chat.messages[0].content).toBe(stored.messages[0].content);
    expect(chat.messages[0].status).toBe('complete');
    expect(chat.sending).toBe(false);
  },
);

it('keeps the original answer available when continuation fails', async () => {
  vi.mocked(api.continueResponse).mockRejectedValue(new Error('engine failed'));
  await chat.continueResponse();
  expect(chat.messages[0].content).toBe('The first half');
  expect(chat.messages[0].status).toBe('truncated');
  expect(chat.sending).toBe(false);
  expect(chat.waitingForContinuation).toBe(false);
  expect(chat.error).not.toBeNull();
});

it('shows preparation until continuation text arrives and clears it on cancellation', async () => {
  vi.mocked(api.continueResponse).mockResolvedValue({
    request_id: 'unused',
    user_message_id: 'user',
    assistant_message_id: 'assistant',
  });
  await chat.continueResponse();
  expect(chat.waitingForContinuation).toBe(true);
  const requestId = chat.requestId!;
  chat.handleDelta({ request_id: requestId, delta: '', reasoning_delta: null, index: 0 });
  expect(chat.waitingForContinuation).toBe(true);
  chat.handleDelta({ request_id: requestId, delta: ' continued', reasoning_delta: null, index: 1 });
  expect(chat.waitingForContinuation).toBe(false);
  const stored = conversation();
  stored.messages[0].status = 'interrupted';
  vi.mocked(api.getConversation).mockResolvedValue(stored);
  await chat.handleCancelled({ request_id: requestId });
  expect(chat.sending).toBe(false);
  expect(chat.messages[0].status).toBe('interrupted');
});

it('ends the continuation indicator on a stalled-engine error', async () => {
  await chat.continueResponse();
  const stored = conversation();
  vi.mocked(api.getConversation).mockResolvedValue(stored);
  await chat.handleError({
    request_id: chat.requestId!,
    error: {
      code: 'engine_crash', message: 'Engine stopped responding',
      detail: null, recovery: ['restart_engine'], retryable: true,
    },
  });
  expect(chat.waitingForContinuation).toBe(false);
  expect(chat.sending).toBe(false);
  expect(chat.messages[0].content).toBe('The first half');
  expect(chat.messages[0].status).toBe('truncated');
  expect(chat.error?.code).toBe('engine_crash');
});
