import sys
import os

sys.stdout.reconfigure(encoding='utf-8')

# Add current directory to path
sys.path.append(os.path.dirname(os.path.abspath(__file__)))

import screen_reader
import subprocess
import time
import uiautomation as auto

def test_ui_tree():
    print("Opening Notepad to test UI extraction...")
    
    # Open notepad.exe
    proc = subprocess.Popen(["notepad.exe"])
    time.sleep(2) # Wait for it to open and become active
    
    print("Extracting UI Tree...")
    # Instead of foreground, try to find Notepad specifically
    notepad = auto.WindowControl(searchDepth=1, ClassName='Notepad')
    if not notepad.Exists(0, 0):
        # Maybe Windows 11 notepad (Notepad is a UWP app now)
        notepad = auto.WindowControl(searchDepth=1, Name='Untitled - Notepad')
        if not notepad.Exists(0,0):
             notepad = auto.GetForegroundControl()

    if notepad:
        print(f"Target found: {notepad.Name}")
        tree_lines = []
        tree_lines.append(f"Window: [{notepad.ControlTypeName}] '{notepad.Name}' @ {notepad.BoundingRectangle}")
        
        for control, depth in auto.WalkControl(notepad, includeTop=False, maxDepth=6):
            if control:
                try:
                    name = control.Name
                    control_type = control.ControlTypeName
                    if not name and control_type in ['PaneControl', 'GroupControl']: continue
                    
                    rect = control.BoundingRectangle
                    w = rect.right - rect.left
                    h = rect.bottom - rect.top
                    if w > 0 and h > 0:
                        tree_lines.append(f"{'  ' * depth}[{control_type}] '{name}' | Center: ({rect.left + w//2}, {rect.top + h//2}) | Size: {w}x{h}")
                except:
                    pass
        tree = "\n".join(tree_lines)
    else:
        tree = "Notepad not found."
        
    print("================ UI TREE ================")
    # Print with utf-8 encoding replacement to console to avoid charmap errors
    print(tree.encode('utf-8', 'replace').decode('utf-8'))
    print("=========================================")
    
    if "Notepad" in tree or "Edit" in tree or "Document" in tree:
        print("\nSUCCESS: Notepad UI detected in the tree!")
    else:
        print("\nWARNING: Could not explicitly find 'Notepad' in the tree.")
        
    print("Closing Notepad...")
    proc.terminate()
    os.system("taskkill /f /im notepad.exe >nul 2>&1")

if __name__ == "__main__":
    test_ui_tree()
