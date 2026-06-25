import sys
import os
import time

sys.path.append(os.path.dirname(os.path.abspath(__file__)))

import screen_reader
import controller

class ComputerUseAgent:
    def __init__(self, llm_inference_callback=None):
        """
        Initializes the agent.
        :param llm_inference_callback: A function that takes (prompt, ui_tree) and returns an action dict.
                                       If None, a dummy human-in-the-loop callback is used for testing.
        """
        self.llm_inference_callback = llm_inference_callback or self._dummy_human_callback
        
    def _dummy_human_callback(self, prompt, ui_tree):
        print("\n[LLM MOCK] Agent needs to decide next action.")
        print("Available actions: click(x,y), type(text), wait(seconds), done()")
        action_str = input("Enter action (e.g., click(100,200) or done()): ").strip()
        
        if action_str.startswith("click"):
            # extremely basic parsing
            coords = action_str.replace("click(", "").replace(")", "").split(",")
            if len(coords) == 2:
                return {"type": "click", "x": int(coords[0]), "y": int(coords[1])}
        elif action_str.startswith("type"):
            text = action_str.replace("type(", "")[:-1]
            return {"type": "type", "text": text}
        elif action_str.startswith("wait"):
            secs = action_str.replace("wait(", "").replace(")", "")
            return {"type": "wait", "seconds": float(secs)}
        elif action_str == "done()":
            return {"type": "done"}
            
        return {"type": "wait", "seconds": 2}

    def execute_task(self, task_instruction):
        print(f"=== Starting Task: {task_instruction} ===")
        print("Press F12 at any time to PANIC ABORT.")
        
        listener = controller.start_panic_listener()
        
        try:
            step = 1
            while True:
                controller.check_panic()
                print(f"\n--- Step {step} ---")
                
                # 1. Read Screen
                print("Reading screen UI...")
                ui_tree = screen_reader.capture_active_window_ui()
                
                # 2. Get Decision from LLM
                print("Asking LLM for next action...")
                action = self.llm_inference_callback(task_instruction, ui_tree)
                
                # 3. Execute Action
                action_type = action.get("type")
                if action_type == "click":
                    x = action.get("x")
                    y = action.get("y")
                    controller.click(x, y)
                elif action_type == "type":
                    text = action.get("text")
                    controller.type_text(text)
                elif action_type == "wait":
                    secs = action.get("seconds", 1)
                    print(f"Waiting for {secs} seconds...")
                    time.sleep(secs)
                elif action_type == "done":
                    print("Task marked as DONE by LLM.")
                    break
                else:
                    print(f"Unknown action type: {action_type}. Waiting 2 seconds.")
                    time.sleep(2)
                    
                step += 1
                time.sleep(1) # Small delay between steps
                
        except InterruptedError as e:
            print(f"\n[ABORTED] {e}")
        except Exception as e:
            print(f"\n[ERROR] An unexpected error occurred: {e}")
        finally:
            listener.stop()
            print("=== Task Execution Ended ===")

if __name__ == "__main__":
    agent = ComputerUseAgent()
    agent.execute_task("Mở Notepad, gõ chữ 'Hello LIVA' và lưu lại ra Desktop")
