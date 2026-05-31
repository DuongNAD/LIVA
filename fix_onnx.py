import onnx
model = onnx.load('e:/Project/LIVA/liva-ui/public/models/hey_liva.onnx')
input_tensor = model.graph.input[0]
print('Old shape:', input_tensor.type.tensor_type.shape)
input_tensor.type.tensor_type.shape.Clear()
dim1 = input_tensor.type.tensor_type.shape.dim.add()
dim1.dim_value = 1
dim2 = input_tensor.type.tensor_type.shape.dim.add()
dim2.dim_value = 16
print('New shape:', input_tensor.type.tensor_type.shape)
onnx.checker.check_model(model)
onnx.save(model, 'e:/Project/LIVA/liva-ui/public/models/hey_liva_fixed.onnx')
print('Saved!')
