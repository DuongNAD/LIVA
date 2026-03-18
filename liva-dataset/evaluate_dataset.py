import json
import statistics
import re
from rich.console import Console

console = Console()
dataset_files = ['train_zalo_tool.jsonl', 'validation_zalo_tool.jsonl']

def evaluate_dataset(file_path):
    valid_count = 0
    error_count = 0
    total_samples = 0
    
    positive_flows = 0   # Read Emails -> Zalo Tool
    negative_flows = 0   # Other tools or No tools
    
    role_sequence_errors = 0
    json_parse_errors = 0
    
    message_lengths = []
    tool_call_lengths = []
    
    console.print(f"[bold cyan]Đang chạy Validation Script V13 trên Dataset: {file_path}[/bold cyan]...\n")
    
    try:
        with open(file_path, 'r', encoding='utf-8') as f:
            for line_no, line in enumerate(f, 1):
                total_samples += 1
                try:
                    item = json.loads(line)
                    messages = item.get("messages", [])
                    
                    if not messages:
                        console.print(f"[red]Lỗi dòng {line_no}: Không tìm thấy mảng 'messages'.[/red]")
                        error_count += 1
                        continue
                        
                    # 1. Kiểm tra cấu trúc hội thoại cơ bản
                    if messages[0].get("role") != "system":
                        role_sequence_errors += 1
                    if len(messages) > 1 and messages[1].get("role") != "user":
                        role_sequence_errors += 1
                        
                    has_read_email = False
                    has_zalo = False
                    
                    # 2. Extract XML <tool_call> tags from Assistant Content
                    for msg in messages:
                        if msg.get("role") == "assistant":
                            content = msg.get("content", "")
                            message_lengths.append(len(content))
                            
                            # Nếu có old format `tool_calls` array (sai chuẩn Qwen)
                            if "tool_calls" in msg:
                                json_parse_errors += 1
                                console.print(f"[red]LỖI NGHIÊM TRỌNG dòng {line_no}: Còn sót key 'tool_calls' kiểu OpenAI, phải dùng thẻ XML![/red]")
                                
                            # Pattern extracted specifically for <tool_call> block
                            matches = re.findall(r'<tool_call>(.*?)</tool_call>', content, re.DOTALL)
                            for match in matches:
                                tc_str = match.strip()
                                tool_call_lengths.append(len(tc_str))
                                
                                try:
                                    tc_obj_or_list = json.loads(tc_str)
                                    tc_list = tc_obj_or_list if isinstance(tc_obj_or_list, list) else [tc_obj_or_list]
                                    
                                    for tc_obj in tc_list:
                                        func_name = tc_obj.get("name", "")
                                        
                                        if func_name == "read_emails":
                                            has_read_email = True
                                        elif func_name == "send_zalo_bot":
                                            has_zalo = True
                                        
                                        # Anti-hallucination check
                                        if func_name not in ["read_emails", "send_zalo_bot", "get_system_time", "search_drive"]:
                                            json_parse_errors += 1
                                            console.print(f"[red]CẢNH BÁO Hallucination {line_no}: Zalo-Bot gọi hàm ma '{func_name}'[/red]")

                                except json.JSONDecodeError:
                                    json_parse_errors += 1
                                    console.print(f"[red]LỖI CÚ PHÁP XML dòng {line_no}: Tool Argument hỏng JSON -> {tc_str}[/red]")
                    
                    # Phân loại kịch bản
                    if has_read_email or has_zalo:
                        positive_flows += 1
                    else:
                        negative_flows += 1
                        
                    # Mẫu hợp lệ khi không dính lỗi cấu trúc nghiêm trọng
                    if json_parse_errors == 0 and role_sequence_errors == 0:
                        valid_count += 1
                    else:
                        error_count += 1

                except json.JSONDecodeError:
                    error_count += 1

    except FileNotFoundError:
        console.print("[bold red]Không tìm thấy file dataset![/bold red]")
        return

    # --- TÍNH ĐIỂM CHẤT LƯỢNG (Scoring) ---
    score = 100.0
    error_penalty = (error_count / total_samples) * 100 * 2
    score -= error_penalty
    
    pos_ratio = positive_flows / total_samples
    if pos_ratio > 0.85:
        score -= 5
    elif pos_ratio < 0.40:
        score -= 10

    score -= (json_parse_errors * 10)
    score = max(0, min(100, score))

    # --- IN BÁO CÁO ---
    console.print(f"--- BÁO CÁO V13 DATASET ({file_path}) ---", style="bold green")
    console.print(f"Tổng mẫu kịch bản  : [bold]{total_samples}[/bold]")
    console.print(f"Mẫu XML hợp lệ     : [bold green]{valid_count}[/bold green] ({(valid_count/total_samples)*100:.1f}%)")
    console.print(f"Lỗi rác XML/JSON   : [bold red]{json_parse_errors}[/bold red]")
    console.print(f"- Luồng XML Tools (P)  : {positive_flows} ({pos_ratio*100:.1f}%)")
    console.print(f"- Luồng Native Text (N): {negative_flows} ({(negative_flows/total_samples)*100:.1f}%)")
    
    if score >= 90:
        grade = "[bold green]A+ (HOÀN HẢO)[/bold green]"
    elif score >= 75:
        grade = "[bold yellow]B (KHÁ TỐT)[/bold yellow]"
    else:
        grade = "[bold red]F (NGUY HIỂM)[/bold red]"
        
    console.print(f"==========================================")
    console.print(f"=> CẤU TRÚC QWEN XML : {grade} - {score:.1f} / 100 điểm")
    console.print(f"==========================================")

if __name__ == "__main__":
    for f in dataset_files:
        evaluate_dataset(f)
        console.print("\n\n")
