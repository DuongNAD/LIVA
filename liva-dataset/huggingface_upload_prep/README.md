---
license: apache-2.0
language:
- vi
- en
tags:
- unsloth
- qwen
- tool-calling
- zalo-bot
- liva-ai
---

# LIVA-Qwen2.5-7B Tool Calling (LoRA)

This model was specifically fine-tuned on top of **Qwen/Qwen2.5-7B-Instruct** using [Unsloth](https://github.com/unslothai/unsloth) to master tool-calling capabilities (Function Calling) specifically tailored for the LIVA AI Engine ecosystem, with a deep focus on Zalo Bot Integration and internal API parsing.

## Model Description

- **Base Model:** Qwen/Qwen2.5-7B-Instruct
- **Fine-Tuning Method:** LoRA (Low-Rank Adaptation)
- **Quantization:** Available in Q8_0 GGUF
- **Dataset Size:** ~2MB of meticulously formatted JSONL conversational data focused on Tool Calling scenarios.
- **Language:** Vietnamese & English
- **Primary Use Case:** Function/Tool Calling, System Automation, Zalo Bot Messaging

## Training Parameters (LoRA & Training-Args)

The following hyperparameters were used during fine-tuning:

* `lora_r`: 16
* `lora_alpha`: 16
* `lora_dropout`: 0
* `target_modules`: `["q_proj", "k_proj", "v_proj", "o_proj", "gate_proj", "up_proj", "down_proj"]`
* `learning_rate`: 2e-4
* `lr_scheduler_type`: linear
* `warmup_ratio`: 0.1
* `max_grad_norm`: 0.3
* `per_device_train_batch_size`: 2
* `gradient_accumulation_steps`: 4
* `num_train_epochs`: 2.0 (1128 steps)
* `weight_decay`: 0.01
* `optimizer`: adamw_8bit
* `max_seq_len`: 2048

## Evaluation & Metrics

The model demonstrates a high resistance to hallucination and strict adherence to `<tool_call>` XML XML structures required by the LIVA AI engine.

* **Tool Call Accuracy:** Excellent structure formatting and intent matching.
* **Safe Rejection Rate:** 100% (Correctly rejects capabilities beyond its predefined tools without attempting to hallucinate parameters).
* **Clarification:** Identifies missing required arguments (e.g. missing message content or parameters) and requests user input before calling the tool.
* **Training Stable:** Smooth convergence recorded over 1128 steps. No exploding gradients.

### Included Assets in this Repository:
* `training_loss_chart.png`: Loss curve over the entire fine-tuning duration.
* `training_detailed_metrics.png`: A comprehensive 4-panel dashboard containing Loss, Learning Rate Schedule, Gradient Norm, and Epoch progression.
* `manual_test_results.txt`: A verifiable log of 10 edge-case testing interactions demonstrating function call logic handling.
* `train_param_script.py`: The Unsloth configuration script used.

*Note: The unmerged LoRA adapter weights and the pre-compiled Q8_0 GGUF files (`Modelfile` and `qwen2.5-7b-instruct.Q8_0.gguf`) are meant to be uploaded alongside this model card.*
