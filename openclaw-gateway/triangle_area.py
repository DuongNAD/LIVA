import math

def calculate_triangle_area(a, b, c):
    # Tính nửa chu vi (semi-perimeter)
    s = (a + b + c) / 2
    
    # Áp dụng công thức Heron
    try:
        area = math.sqrt(s * (s - a) * (s - b) * (s - c))
        return area
    except ValueError:
        return "Lỗi: Ba cạnh này không tạo thành một tam giác hợp lệ."

# Các cạnh đã cho
a = 3
b = 4
c = 5

# Tính và in kết quả
area = calculate_triangle_area(a, b, c)
print(f"Diện tích tam giác với 3 cạnh {a}, {b}, {c} là: {area}")