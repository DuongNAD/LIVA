export type DiffLineType = 'context' | 'addition' | 'deletion';

export interface DiffLine {
  line_type: DiffLineType;
  content: string;
  old_line_no: number | null;
  new_line_no: number | null;
}

export type HunkStatus =
  | { type: 'pending' }
  | { type: 'approved' }
  | { type: 'rejected'; payload?: { reason?: string } }
  | { type: 'modified'; payload: { user_override: string } };

export interface DiffHunk {
  hunk_id: string;
  file_path: string;
  old_start: number;
  old_lines: number;
  new_start: number;
  new_lines: number;
  header: string;
  lines: DiffLine[];
  diff_content: string;
  status: HunkStatus;
}

export interface FileDiff {
  old_path: string | null;
  new_path: string | null;
  is_new: boolean;
  is_deleted: boolean;
  is_renamed: boolean;
  hunks: DiffHunk[];
}

export type DiffReviewStatus =
  | 'pending'
  | 'fully_approved'
  | 'partially_approved'
  | 'rejected'
  | 'applied';

export interface DiffReviewSession {
  session_id: string;
  thread_id: string;
  action_id: string;
  files: FileDiff[];
  status: DiffReviewStatus;
  created_at: number;
  updated_at: number;
}
