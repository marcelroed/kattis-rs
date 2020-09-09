from math import sqrt
T = int(input())

for _ in range(T):
    x, y = map(int, input().split())

    h = (x + y - sqrt(x**2 - x * y + y**2)) / 6
    a = x - 2 * h
    b = y - 2 * h
    v = h * a * b
    print(v)