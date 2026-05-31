import React, { useState, useRef, Suspense, startTransition } from 'react';
import { Dialog, DialogTitle, AppBar, Toolbar, IconButton, Typography, Button, Box, CircularProgress } from '@mui/material';
import { X, Check } from 'lucide-react';

// 1. Lazy-load the ProseMirror/TipTap editor to minimize main bundle bloat
const VaultEditor = React.lazy(() => import('../components/VaultEditor'));

interface VaultReaderModalProps {
    open: boolean;
    onClose: () => void;
    vaultItemId: number;
    initialMarkdown: string;
    originalMarkdown?: string | null;
}

export const VaultReaderModal: React.FC<VaultReaderModalProps> = ({
    open,
    onClose,
    vaultItemId,
    initialMarkdown,
    originalMarkdown,
}) => {
    const [isDirty, setIsDirty] = useState(false);
    const [markdownContent, setMarkdownContent] = useState(initialMarkdown);
    const saveTriggerRef = useRef<(() => void) | null>(null);

    // 2. Gate closing on unsaved changes
    const handleCloseAttempt = () => {
        if (isDirty) {
            const confirmClose = window.confirm('You have unsaved changes. Discard them?');
            if (!confirmClose) return;
        }
        setIsDirty(false);
        onClose();
    };

    // 3. Initiate ProseMirror -> markdown save cascade via trigger ref
    const triggerSave = () => {
        if (saveTriggerRef.current) {
            saveTriggerRef.current();
        }
    };

    const handleSaveSuccess = (updatedMarkdown: string) => {
        setMarkdownContent(updatedMarkdown);
        setIsDirty(false);
    };

    const handleRevertSuccess = (originalContent: string) => {
        setMarkdownContent(originalContent);
        setIsDirty(false);
    };

    return (
        <Dialog fullScreen open={open} onClose={handleCloseAttempt}>
            <AppBar sx={{ position: 'relative', bgcolor: 'background.paper' }}>
                <Toolbar>
                    <IconButton edge="start" color="inherit" onClick={handleCloseAttempt} aria-label="close">
                        <X size={20} />
                    </IconButton>
                    <Typography sx={{ ml: 2, flex: 1 }} variant="h6" component="div">
                        Edit Vault Item
                    </Typography>
                    
                    {/* 4. Display a dynamic save indicator button when editor is dirty */}
                    {isDirty && (
                        <Button
                            variant="contained"
                            color="primary"
                            startIcon={<Check size={16} />}
                            onClick={triggerSave}
                            sx={{ borderRadius: 99 }}
                        >
                            Save Changes
                        </Button>
                    )}
                </Toolbar>
            </AppBar>

            <Box sx={{ display: 'flex', flexDirection: 'column', flexGrow: 1, overflow: 'hidden' }}>
                {/* 5. Wrap the lazy components in a clean Suspense fallback loader */}
                <Suspense
                    fallback={
                        <Box sx={{ display: 'flex', justifyContent: 'center', alignItems: 'center', height: '100%' }}>
                            <CircularProgress size={32} />
                        </Box>
                    }
                >
                    <VaultEditor
                        vaultItemId={vaultItemId}
                        initialMarkdown={markdownContent}
                        originalMarkdown={originalMarkdown}
                        onDirtyChange={setIsDirty}
                        onSave={handleSaveSuccess}
                        onRevert={handleRevertSuccess}
                        saveRef={saveTriggerRef}
                        t={(key) => key} // Identity mock for translating
                    />
                </Suspense>
            </Box>
        </Dialog>
    );
};
