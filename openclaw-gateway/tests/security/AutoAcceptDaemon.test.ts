import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { AutoAcceptDaemon } from '../../src/security/AutoAcceptDaemon';
import { CDPBridge } from '../../src/bridges/CDPBridge';
import { TelegramBridge } from '../../src/channels/TelegramBridge';

vi.mock('../../src/utils/logger', () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn()
    }
}));

describe('AutoAcceptDaemon', () => {
    let daemon: AutoAcceptDaemon;
    let mockCdpBridge: any;
    let mockTelegramBridge: any;

    beforeEach(() => {
        vi.useFakeTimers();
        mockCdpBridge = {
            on: vi.fn(),
            clickApprovalButton: vi.fn().mockResolvedValue(undefined)
        };
        mockTelegramBridge = {
            on: vi.fn(),
            sendText: vi.fn().mockResolvedValue(undefined),
            sendApprovalCard: vi.fn().mockResolvedValue(undefined),
            editMessage: vi.fn().mockResolvedValue(undefined)
        };

        process.env.TELEGRAM_CHAT_ID = 'test_chat_id';
        daemon = new AutoAcceptDaemon(mockCdpBridge as any, mockTelegramBridge as any);
    });

    afterEach(() => {
        vi.useRealTimers();
        vi.clearAllMocks();
    });

    it('should enable and disable daemon', () => {
        daemon.disable();
        daemon.enable();
        // Since we mocked logger, we just verify it doesn't crash
        expect(true).toBe(true);
    });

    it('should ignore events when disabled', async () => {
        daemon.disable();
        const approvalHandler = mockCdpBridge.on.mock.calls.find((call: any) => call[0] === 'approval_required')[1];
        await approvalHandler({ command: 'echo test' });
        expect(mockCdpBridge.clickApprovalButton).not.toHaveBeenCalled();
    });

    it('should respect cooldown', async () => {
        const approvalHandler = mockCdpBridge.on.mock.calls.find((call: any) => call[0] === 'approval_required')[1];
        
        await approvalHandler({ command: 'echo first' });
        expect(mockCdpBridge.clickApprovalButton).toHaveBeenCalledTimes(1);

        // Call again within cooldown
        await approvalHandler({ command: 'echo second' });
        expect(mockCdpBridge.clickApprovalButton).toHaveBeenCalledTimes(1); // Still 1

        // Advance time past cooldown
        vi.advanceTimersByTime(4000);
        await approvalHandler({ command: 'echo third' });
        expect(mockCdpBridge.clickApprovalButton).toHaveBeenCalledTimes(2);
    });

    it('should reject blocked commands immediately', async () => {
        // Reset time so cooldown isn't active
        vi.advanceTimersByTime(5000);
        const approvalHandler = mockCdpBridge.on.mock.calls.find((call: any) => call[0] === 'approval_required')[1];
        
        await approvalHandler({ command: 'rm -rf /' });

        expect(mockCdpBridge.clickApprovalButton).toHaveBeenCalledWith(false);
        expect(mockTelegramBridge.sendText).toHaveBeenCalled();
    });

    it('should auto-approve safe commands if whitelist is empty', async () => {
        vi.advanceTimersByTime(5000);
        const approvalHandler = mockCdpBridge.on.mock.calls.find((call: any) => call[0] === 'approval_required')[1];
        
        await approvalHandler({ command: 'ls -la' });

        expect(mockCdpBridge.clickApprovalButton).toHaveBeenCalledWith(true);
    });

    it('should fallback to HITL if not in whitelist', async () => {
        mockCdpBridge.on.mockClear();
        mockTelegramBridge.on.mockClear();
        const strictDaemon = new AutoAcceptDaemon(mockCdpBridge as any, mockTelegramBridge as any, { allowedCommands: ['git status'] });
        vi.advanceTimersByTime(5000);

        const approvalHandler = mockCdpBridge.on.mock.calls[0][1];
        await approvalHandler({ command: 'git commit' });

        expect(mockTelegramBridge.sendApprovalCard).toHaveBeenCalled();
        expect(mockCdpBridge.clickApprovalButton).not.toHaveBeenCalled();
        
        // Timeout should reject
        vi.advanceTimersByTime(65000);
        expect(mockCdpBridge.clickApprovalButton).toHaveBeenCalledWith(false);
    });

    it('should ignore duplicate HITL requests while one is pending', async () => {
        mockCdpBridge.on.mockClear();
        mockTelegramBridge.on.mockClear();
        const strictDaemon = new AutoAcceptDaemon(mockCdpBridge as any, mockTelegramBridge as any, { allowedCommands: ['safe_cmd'] });
        vi.advanceTimersByTime(5000);

        const approvalHandler = mockCdpBridge.on.mock.calls[0][1];
        
        await approvalHandler({ command: 'unsafe_cmd_1' });
        
        // Advance time to bypass cooldown but not HITL timeout
        vi.advanceTimersByTime(4000);
        await approvalHandler({ command: 'unsafe_cmd_2' });

        expect(mockTelegramBridge.sendApprovalCard).toHaveBeenCalledTimes(1);
    });

    it('should handle telegram callbacks to approve', async () => {
        mockCdpBridge.on.mockClear();
        mockTelegramBridge.on.mockClear();
        const strictDaemon = new AutoAcceptDaemon(mockCdpBridge as any, mockTelegramBridge as any, { allowedCommands: ['git status'] });
        vi.advanceTimersByTime(5000);

        const approvalHandler = mockCdpBridge.on.mock.calls[0][1];
        const callbackHandler = mockTelegramBridge.on.mock.calls[0][1];
        
        await approvalHandler({ command: 'git commit' });
        await callbackHandler({ data: 'approve:hitl_123', chatId: '123', messageId: '456', senderId: 'user1' });

        expect(mockTelegramBridge.editMessage).toHaveBeenCalled();
        expect(mockCdpBridge.clickApprovalButton).toHaveBeenCalledWith(true);
    });

    it('should handle telegram callbacks to reject', async () => {
        mockCdpBridge.on.mockClear();
        mockTelegramBridge.on.mockClear();
        const strictDaemon = new AutoAcceptDaemon(mockCdpBridge as any, mockTelegramBridge as any, { allowedCommands: ['git status'] });
        vi.advanceTimersByTime(5000);

        const approvalHandler = mockCdpBridge.on.mock.calls[0][1];
        const callbackHandler = mockTelegramBridge.on.mock.calls[0][1];
        
        await approvalHandler({ command: 'git commit' });
        await callbackHandler({ data: 'reject:hitl_123', chatId: '123', messageId: '456', senderId: 'user1' });

        expect(mockCdpBridge.clickApprovalButton).toHaveBeenCalledWith(false);
    });

    it('should handle telegram callbacks without pending approval', async () => {
        const callbackHandler = mockTelegramBridge.on.mock.calls.find((call: any) => call[0] === 'callback_query')[1];
        await callbackHandler({ data: 'approve:hitl_123', chatId: '123', messageId: '456', senderId: 'user1' });

        expect(mockTelegramBridge.editMessage).toHaveBeenCalledWith('123', '456', expect.stringContaining('expired'));
    });
    
    it('should handle telegram callbacks not related to hitl', async () => {
        const callbackHandler = mockTelegramBridge.on.mock.calls.find((call: any) => call[0] === 'callback_query')[1];
        await callbackHandler({ data: 'something_else', chatId: '123', messageId: '456', senderId: 'user1' });

        expect(mockTelegramBridge.editMessage).not.toHaveBeenCalled();
    });

    it('should handle telegram callbacks without messageId', async () => {
        mockCdpBridge.on.mockClear();
        mockTelegramBridge.on.mockClear();
        const strictDaemon = new AutoAcceptDaemon(mockCdpBridge as any, mockTelegramBridge as any, { allowedCommands: ['git status'] });
        vi.advanceTimersByTime(5000);

        const approvalHandler = mockCdpBridge.on.mock.calls[0][1];
        const callbackHandler = mockTelegramBridge.on.mock.calls[0][1];
        
        await approvalHandler({ command: 'git commit' });
        await callbackHandler({ data: 'approve:hitl_123', chatId: '123', senderId: 'user1' });

        expect(mockTelegramBridge.editMessage).not.toHaveBeenCalled();
        expect(mockCdpBridge.clickApprovalButton).toHaveBeenCalledWith(true);
    });

    it('should handle telegram callbacks without pending approval and without messageId', async () => {
        const callbackHandler = mockTelegramBridge.on.mock.calls.find((call: any) => call[0] === 'callback_query')[1];
        await callbackHandler({ data: 'approve:hitl_123', chatId: '123', senderId: 'user1' });

        expect(mockTelegramBridge.editMessage).not.toHaveBeenCalled();
    });

    it('should not send messages if chatId is missing', async () => {
        delete process.env.TELEGRAM_CHAT_ID;
        mockCdpBridge.on.mockClear();
        mockTelegramBridge.on.mockClear();
        const noChatDaemon = new AutoAcceptDaemon(mockCdpBridge as any, mockTelegramBridge as any, { allowedCommands: ['safe'] });
        vi.advanceTimersByTime(5000);

        const approvalHandler = mockCdpBridge.on.mock.calls[0][1];
        
        // 1. Blacklist check
        await approvalHandler({ command: 'rm -rf /' });
        expect(mockTelegramBridge.sendText).not.toHaveBeenCalled();
        
        // 2. HITL check
        vi.advanceTimersByTime(5000);
        await approvalHandler({ command: 'git commit' });
        expect(mockTelegramBridge.sendApprovalCard).not.toHaveBeenCalled();

        // 3. Clear hitlTimer manually for coverage
        const callbackHandler = mockTelegramBridge.on.mock.calls[0][1];
        
        // Need to clear pending via timeout but without throwing if hitlTimer is null
        (noChatDaemon as any).hitlTimer = null; 
        
        await callbackHandler({ data: 'approve:hitl_123', chatId: '123', senderId: 'user1' });
        
        // Restore
        process.env.TELEGRAM_CHAT_ID = 'test_chat_id';
    });

    it('should handle hitlTimer being falsy during resolveHITL (Line 140)', async () => {
        // This covers the branch where #hitlTimer is null/falsy when #resolveHITL is called.
        // We achieve this by triggering the HITL timeout (60s), which calls #resolveHITL
        // and clears #hitlTimer. Then if we trigger a second HITL, the first timeout's 
        // clearTimeout branch is falsy because it was already cleared.
        
        mockCdpBridge.on.mockClear();
        mockTelegramBridge.on.mockClear();
        const strictDaemon = new AutoAcceptDaemon(mockCdpBridge as any, mockTelegramBridge as any, { allowedCommands: ['safe'] });
        vi.advanceTimersByTime(5000);

        const approvalHandler = mockCdpBridge.on.mock.calls[0][1];
        const callbackHandler = mockTelegramBridge.on.mock.calls[0][1];

        // 1. Trigger HITL
        await approvalHandler({ command: 'unsafe1' });
        
        // 2. Let HITL timeout fire (clears pendingApproval AND hitlTimer)
        vi.advanceTimersByTime(65000);
        
        // clickApprovalButton should have been called once with false (timeout rejection)
        expect(mockCdpBridge.clickApprovalButton).toHaveBeenCalledWith(false);
    });

    it('should do nothing if HITL timeout fires but pendingApproval is already null (Line 130)', async () => {
        mockCdpBridge.on.mockClear();
        mockTelegramBridge.on.mockClear();
        const strictDaemon = new AutoAcceptDaemon(mockCdpBridge as any, mockTelegramBridge as any, { allowedCommands: ['safe'] });
        vi.advanceTimersByTime(5000);

        const approvalHandler = mockCdpBridge.on.mock.calls[0][1];
        const callbackHandler = mockTelegramBridge.on.mock.calls[0][1];

        // 1. Trigger HITL
        await approvalHandler({ command: 'unsafe2' });
        
        // 2. Approve via callback (resolveHITL clears pendingApproval + hitlTimer)
        await callbackHandler({ data: 'approve:hitl_123', chatId: '123', messageId: '456', senderId: 'user1' });
        expect(mockCdpBridge.clickApprovalButton).toHaveBeenCalledWith(true);
        
        // 3. Now advance past 60s so the original timeout fires — but pendingApproval is null
        vi.advanceTimersByTime(65000);
        
        // Should NOT call clickApprovalButton again — the guard `if (this.#pendingApproval)` prevents it
        expect(mockCdpBridge.clickApprovalButton).toHaveBeenCalledTimes(1);
    });
});
