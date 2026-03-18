import json
import matplotlib.pyplot as plt

# Load the trainer state from the final checkpoint
log_path = 'outputs/checkpoint-1128/trainer_state.json'
try:
    with open(log_path, 'r', encoding='utf-8') as f:
        data = json.load(f)
except Exception as e:
    print(f"Error reading {log_path}: {e}")
    exit(1)

steps = []
losses = []

# Parse the log history array for loss values
for log in data.get('log_history', []):
    if 'loss' in log and 'step' in log:
        steps.append(log['step'])
        losses.append(log['loss'])

if not steps:
    print("No loss data found in the logs.")
    exit(1)

# Plotting the data
plt.figure(figsize=(10, 6))
plt.plot(steps, losses, label='Training Loss', color='#1f77b4', linewidth=2)

plt.xlabel('Training Steps', fontsize=12)
plt.ylabel('Loss', fontsize=12)
plt.title('LIVA-Qwen2.5-7B Tool Calling Fine-Tuning Loss Curve', fontsize=14, pad=15)

plt.grid(True, linestyle='--', alpha=0.7)
plt.legend(fontsize=12)
plt.tight_layout()

# Save the plot
output_file = 'training_loss_chart.png'
plt.savefig(output_file, dpi=300)
print(f"✅ Loss chart generated successfully and saved to {output_file}")
