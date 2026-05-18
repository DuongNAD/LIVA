/**
 * Centralized UI Locators for CDP Interaction.
 * Decouples CSS selectors from business logic to withstand IDE UI updates.
 */
export const CDPUILocators = {
    // Inputs
    chatInput: 'textarea, [contenteditable="true"]',
    
    // Approval UI
    approvalButtons: 'button, [role="button"], a',
    approvalTextPatterns: /^(Run|Allow|Accept|Approve|Chấp nhận|Cho phép)$/i,
    rejectTextPatterns: /^(Reject|Deny|Cancel|Từ chối|Hủy)$/i,
    
    // Threads
    activeThreads: '.thread-list-item',
    
    // Content extraction
    latestResponse: '.response-content:last-of-type'
};
