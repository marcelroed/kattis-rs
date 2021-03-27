from sys import stdin
from collections import deque
import io
import os

input = io.BytesIO(os.read(0, os.fstat(0).st_size)).readline

front = deque()
back = deque()

out = []

N = int(input().decode())

for _ in range(N):
    line = input().decode()
    op, opnd = line.split()
    opnd = int(opnd)
    if op[0] == 'g':
        if opnd >= len(front):
            out.append(back[opnd - len(front)])
        else:
            out.append(front[opnd])
    elif op[5] == 'b':
        back.append(opnd)
        if len(back) > len(front):
            front.append(back.popleft())
    elif op[5] == 'f':
        front.appendleft(opnd)
        if len(front) > len(back) + 1:
            back.appendleft(front.pop())
    elif op[5] == 'm':
        if len(front) > len(back):
            back.appendleft(opnd)
        else:
            front.append(opnd)

print('\n'.join(map(str, out)))
