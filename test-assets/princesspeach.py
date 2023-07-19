from math import ceil
from sys import stdout
import os
import io

input = io.BytesIO(os.read(0, os.fstat(0).st_size)).readline

N, Y = map(int, input().split())

found = list(map(int, [input() for _ in range(Y)]))

for i in range(N):
    if i not in found:
        stdout.write(f'{i}\n')

stdout.write(f'Mario got {Y} of the dangerous obstacles.')
