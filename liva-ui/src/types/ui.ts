export type ToolPanelState = 'loading' | 'done' | 'error';

export interface ToolPanelView {
  tool: string;
  state: ToolPanelState;
  payload: unknown;
}
