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
lrs = []
epochs = []
grad_norms = []

# Parse the log history array for all values
for log in data.get('log_history', []):
    if 'step' in log and 'loss' in log:
        steps.append(log['step'])
        losses.append(log['loss'])
        lrs.append(log.get('learning_rate', 0))
        epochs.append(log.get('epoch', 0))
        grad_norms.append(log.get('grad_norm', 0))

if not steps:
    print("No log data found.")
    exit(1)

# Create a 2x2 grid of subplots
fig, axs = plt.subplots(2, 2, figsize=(14, 10))
fig.suptitle('LIVA-Qwen2.5-7B Tool Calling Fine-Tuning Overview', fontsize=18, fontweight='bold', y=0.98)

# Plot 1: Training Loss
axs[0, 0].plot(steps, losses, color='#d62728', linewidth=2)
axs[0, 0].set_title('Training Loss vs Steps', fontsize=14)
axs[0, 0].set_xlabel('Steps')
axs[0, 0].set_ylabel('Loss')
axs[0, 0].grid(True, linestyle='--', alpha=0.6)

# Plot 2: Learning Rate
axs[0, 1].plot(steps, lrs, color='#1f77b4', linewidth=3)
axs[0, 1].set_title('Learning Rate Schedule', fontsize=14)
axs[0, 1].set_xlabel('Steps')
axs[0, 1].set_ylabel('Learning Rate')
axs[0, 1].grid(True, linestyle='--', alpha=0.6)

# Plot 3: Gradient Norm
axs[1, 0].plot(steps, grad_norms, color='#2ca02c', linewidth=2)
axs[1, 0].set_title('Gradient Norm vs Steps', fontsize=14)
axs[1, 0].set_xlabel('Steps')
axs[1, 0].set_ylabel('Gradient Norm')
axs[1, 0].grid(True, linestyle='--', alpha=0.6)

# Plot 4: Epoch Progression
axs[1, 1].plot(steps, epochs, color='#ff7f0e', linewidth=2)
axs[1, 1].set_title('Epochs Completed vs Steps', fontsize=14)
axs[1, 1].set_xlabel('Steps')
axs[1, 1].set_ylabel('Epochs')
axs[1, 1].grid(True, linestyle='--', alpha=0.6)

plt.tight_layout(rect=[0, 0, 1, 0.95])

# Save the plot
output_file = 'training_detailed_metrics.png'
plt.savefig(output_file, dpi=300)
print(f"✅ Detailed metrics chart generated successfully: {output_file}")
