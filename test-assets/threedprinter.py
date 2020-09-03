def crossProduct(u, v):
    return [
        u[1]*v[2] - u[2]*v[1],
        u[2]*v[0] - u[0]*v[2],
        u[0]*v[1] - u[1]*v[0]
        ]

def dotProduct(u, v):
    return sum([u[i]*v[i] for i in range(len(u))])

def vectorSub(u, v):
    return [u[i] - v[i] for i in range(len(u))]

def vectorAdd(u, v):
    return [u[i] + v[i] for i in range(len(u))]

polyhedra = int(input())
totalvolume = 0

for _ in range(polyhedra):
    localvolume = 0
    facecount = int(input())
    faces = []
    # Input faces
    for _ in range(facecount):
        vertinput = input().split()
        #print(vertinput)
        vertcount = int(vertinput[0])
        verts = [[float(j) for j in vertinput[1 + i:4 + i]] for i in range(0, len(vertinput) - 1, 3)]
        faces.append(verts)
    # Find a point within the polyhedron
    midvert = faces[0][0]
    # Find volume of convex-polygonal pyramids corresponding to face
    for face in faces:
        reference = face[0]
        for vertex_idx in range(2, len(face)):
            triarea = crossProduct(vectorSub(face[vertex_idx], reference), vectorSub(face[vertex_idx - 1], reference))
            tetravolume = abs(dotProduct(triarea, vectorSub(midvert, reference))/6)
            localvolume += tetravolume
    totalvolume += localvolume
out = "{0:.2f}".format(totalvolume)
print(out)
