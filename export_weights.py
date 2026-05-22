import onnx
import json
from onnx import numpy_helper
model = onnx.load('e:/Project/LIVA/liva-ui/public/models/hey_liva_fixed.onnx')
weights = {}
for init in model.graph.initializer:
    weights[init.name] = numpy_helper.to_array(init).tolist()
with open('e:/Project/LIVA/liva-ui/src/workers/hey_liva_weights.json', 'w') as f:
    json.dump(weights, f)
