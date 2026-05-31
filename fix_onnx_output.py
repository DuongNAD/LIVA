import onnx
model = onnx.load('e:/Project/LIVA/liva-ui/public/models/hey_liva_fixed.onnx')
output_tensor = model.graph.output[0]
print('Old output shape:', output_tensor.type.tensor_type.shape)
output_tensor.type.tensor_type.shape.Clear()
dim1 = output_tensor.type.tensor_type.shape.dim.add()
dim1.dim_value = 1
dim2 = output_tensor.type.tensor_type.shape.dim.add()
dim2.dim_value = 2
print('New output shape:', output_tensor.type.tensor_type.shape)
onnx.checker.check_model(model)
onnx.save(model, 'e:/Project/LIVA/liva-ui/public/models/hey_liva_fixed.onnx')
print('Saved!')
