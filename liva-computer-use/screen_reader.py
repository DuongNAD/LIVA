import uiautomation as auto
import time

def capture_active_window_ui(max_depth=5):
    """
    Captures the UI tree of the currently active foreground window.
    Returns a string representation of the UI elements and their coordinates.
    """
    active_window = auto.GetForegroundControl()
    if not active_window:
        return "No active window found."
        
    tree_lines = []
    
    try:
        window_rect = active_window.BoundingRectangle
        tree_lines.append(f"Active Window: [{active_window.ControlTypeName}] '{active_window.Name}' @ {window_rect}")
    except Exception:
        tree_lines.append("Active Window: Unknown")

    start_time = time.time()
    
    # Walk the tree of the active window
    for control, depth in auto.WalkControl(active_window, includeTop=False, maxDepth=max_depth):
        if time.time() - start_time > 5:  # Timeout to prevent hanging
            tree_lines.append("... [UI Tree Truncated due to timeout] ...")
            break
            
        if not control:
            continue
            
        try:
            name = control.Name
            control_type = control.ControlTypeName
            rect = control.BoundingRectangle
            
            # Skip items that are practically invisible or empty
            if not name and control_type in ['PaneControl', 'GroupControl', 'CustomControl']:
                continue
                
            w = rect.right - rect.left
            h = rect.bottom - rect.top
            
            if w <= 0 or h <= 0:
                continue
                
            # Calculate center coordinates for easy clicking
            center_x = rect.left + (w // 2)
            center_y = rect.top + (h // 2)
            
            line = f"{'  ' * depth}[{control_type}] '{name}' | Center: ({center_x}, {center_y}) | Size: {w}x{h}"
            tree_lines.append(line)
        except Exception:
            pass
            
    return "\n".join(tree_lines)

if __name__ == "__main__":
    print("Capturing Active Window UI Tree in 3 seconds...")
    time.sleep(3)
    tree = capture_active_window_ui()
    print("================ UI TREE ================")
    print(tree)
    print("=========================================")
