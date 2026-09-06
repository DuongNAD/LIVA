import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useLivingCanvas } from '../../src/composables/useLivingCanvas';
import type { DiffReviewSession } from '../../src/types/diff';

const mockInvokeBackend = vi.fn();

vi.mock('../../src/platform', () => ({
  getPlatformAdapter: () => ({
    invokeBackend: (...args: any[]) => mockInvokeBackend(...args),
  }),
}));

describe('useLivingCanvas Composable', () => {
  const sampleSession: DiffReviewSession = {
    session_id: 'sess-test-1',
    thread_id: 'thread-1',
    action_id: 'act-1',
    status: 'pending',
    created_at: 1000,
    updated_at: 1000,
    files: [
      {
        old_path: 'src/main.rs',
        new_path: 'src/main.rs',
        is_new: false,
        is_deleted: false,
        is_renamed: false,
        hunks: [
          {
            hunk_id: 'hunk-1',
            file_path: 'src/main.rs',
            old_start: 1,
            old_lines: 5,
            new_start: 1,
            new_lines: 6,
            header: 'fn main()',
            lines: [
              { line_type: 'context', content: 'use std::fs;', old_line_no: 1, new_line_no: 1 },
              { line_type: 'addition', content: 'use std::io;', old_line_no: null, new_line_no: 2 },
            ],
            diff_content: '@@ -1,5 +1,6 @@\n use std::fs;\n+use std::io;',
            status: { type: 'pending' },
          },
          {
            hunk_id: 'hunk-2',
            file_path: 'src/main.rs',
            old_start: 20,
            old_lines: 4,
            new_start: 21,
            new_lines: 5,
            header: 'fn run()',
            lines: [
              { line_type: 'deletion', content: 'let x = 1;', old_line_no: 20, new_line_no: null },
              { line_type: 'addition', content: 'let x = 2;', old_line_no: null, new_line_no: 21 },
            ],
            diff_content: '@@ -20,4 +21,5 @@\n-let x = 1;\n+let x = 2;',
            status: { type: 'pending' },
          },
        ],
      },
    ],
  };

  beforeEach(() => {
    vi.clearAllMocks();
    const canvas = useLivingCanvas();
    canvas.setLayoutMode('hybrid');
    canvas.setSplitRatio(0.5);
  });

  it('should initialize with default state and compute pending hunks correctly', () => {
    const canvas = useLivingCanvas();
    canvas.setMockSession(JSON.parse(JSON.stringify(sampleSession)));

    expect(canvas.layoutMode.value).toBe('hybrid');
    expect(canvas.splitRatio.value).toBe(0.5);
    expect(canvas.pendingHunksCount.value).toBe(2);
    expect(canvas.isAllDecided.value).toBe(false);
  });

  it('should allow setting layout mode and clamped split ratio', () => {
    const canvas = useLivingCanvas();
    canvas.setLayoutMode('diff');
    expect(canvas.layoutMode.value).toBe('diff');

    canvas.setSplitRatio(0.05); // clamped to min 0.1
    expect(canvas.splitRatio.value).toBe(0.1);

    canvas.setSplitRatio(0.95); // clamped to max 0.9
    expect(canvas.splitRatio.value).toBe(0.9);

    canvas.setSplitRatio(0.6);
    expect(canvas.splitRatio.value).toBe(0.6);
  });

  it('should optimistically update hunk status and invoke backend', async () => {
    const canvas = useLivingCanvas();
    const sessionCopy: DiffReviewSession = JSON.parse(JSON.stringify(sampleSession));
    canvas.setMockSession(sessionCopy);

    mockInvokeBackend.mockResolvedValueOnce({
      session: {
        ...sessionCopy,
        files: [
          {
            ...sessionCopy.files[0],
            hunks: [
              { ...sessionCopy.files[0].hunks[0], status: { type: 'approved' } },
              sessionCopy.files[0].hunks[1],
            ],
          },
        ],
      },
    });

    await canvas.submitHunkDecision('sess-test-1', 'hunk-1', { type: 'approved' });

    expect(mockInvokeBackend).toHaveBeenCalledWith('agent:submit_hunk_decision', {
      session_id: 'sess-test-1',
      hunk_id: 'hunk-1',
      decision: 'approved',
    });

    expect(canvas.pendingHunksCount.value).toBe(1);
  });

  it('should handle batch approve and reject decisions', async () => {
    const canvas = useLivingCanvas();
    const sessionCopy: DiffReviewSession = JSON.parse(JSON.stringify(sampleSession));
    canvas.setMockSession(sessionCopy);

    mockInvokeBackend.mockResolvedValueOnce({
      session: {
        ...sessionCopy,
        status: 'fully_approved',
        files: [
          {
            ...sessionCopy.files[0],
            hunks: sessionCopy.files[0].hunks.map((h) => ({ ...h, status: { type: 'approved' } })),
          },
        ],
      },
    });

    await canvas.approveAllPending();

    expect(mockInvokeBackend).toHaveBeenCalledWith('agent:submit_hunk_decision', {
      session_id: 'sess-test-1',
      batch: 'approve_all',
    });

    expect(canvas.pendingHunksCount.value).toBe(0);
    expect(canvas.isAllDecided.value).toBe(true);
  });

  it('should handle canvas widget state updates and actions', async () => {
    const canvas = useLivingCanvas();
    mockInvokeBackend.mockResolvedValueOnce({ status: 'ok' });

    await canvas.sendWidgetAction('widget-01', 'button_click', { action_type: 'refresh' });

    expect(mockInvokeBackend).toHaveBeenCalledWith('canvas:update_widget_state', {
      widget_id: 'widget-01',
      props: {
        action: 'button_click',
        payload: { action_type: 'refresh' },
      },
    });
  });
});
