import React from 'react';
import { Box, Typography, Button, Tooltip, IconButton, Grid } from '@mui/material';
import { Trash2 } from 'lucide-react';

interface UserRow {
    id: number;
    email: string;
    role: 'admin' | 'user';
}

interface AdminUserListProps {
    users: UserRow[];
    onDeleteClick: (id: number) => void;
}

export const AdminUserList: React.FC<AdminUserListProps> = ({ users, onDeleteClick }) => {
    return (
        <Box sx={{ flexGrow: 1, p: 3 }}>
            {/* 1. Standard MUI v7 Grid2 API using the 'size' object prop */}
            <Grid container spacing={2}>
                {users.map((user) => (
                    <Grid key={user.id} size={{ xs: 12, md: 6, lg: 4 }}>
                        <Box
                            sx={{
                                p: 2,
                                borderRadius: 2,
                                bgcolor: 'background.paper',
                                border: '1px solid rgba(255,255,255,0.08)',
                                display: 'flex',
                                justifyContent: 'space-between',
                                alignItems: 'center',
                            }}
                        >
                            <Box>
                                <Typography variant="body1" fontWeight={600}>
                                    {user.email}
                                </Typography>
                                <Typography variant="caption" color="text.secondary">
                                    Role: {user.role.toUpperCase()}
                                </Typography>
                            </Box>

                            {/* 2. GDPR Deletion Gating: Disables delete button for 'admin' role */}
                            {user.role === 'admin' ? (
                                <Tooltip title="Admin accounts cannot be deleted. Demote their role to User first." placement="top">
                                    <span>
                                        <IconButton disabled color="error" sx={{ opacity: 0.35 }}>
                                            <Trash2 size={18} />
                                        </IconButton>
                                    </span>
                                </Tooltip>
                            ) : (
                                <Tooltip title="Permanently delete user profile" placement="top">
                                    <IconButton onClick={() => onDeleteClick(user.id)} color="error">
                                        <Trash2 size={18} />
                                    </IconButton>
                                </Tooltip>
                            )}
                        </Box>
                    </Grid>
                ))}
            </Grid>
        </Box>
    );
};
