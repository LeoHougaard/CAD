"""Bounded half-edge walks. Native endpoint/tangent data supplied by caller."""
import math

def cycles(edges):
    # edges: (persistent ID, start vertex, end vertex, start angle, end angle)
    outgoing = {}
    for gid,a,b,ta,tb in edges:
        outgoing.setdefault(a, []).append((ta, gid, 0))
        outgoing.setdefault(b, []).append((tb, gid, 1))
    for rows in outgoing.values():
        rows.sort()
        if any(abs(rows[i][0]-rows[i-1][0]) < 1e-12 for i in range(1,len(rows))):
            raise ValueError('Ambiguous tangent ordering at a native junction')
    ends = {(gid,0):b for gid,a,b,_,_ in edges}
    ends.update({(gid,1):a for gid,a,b,_,_ in edges})
    successor = {}
    for h,vertex in ends.items():
        rows = outgoing[vertex]
        reverse = (h[0],1-h[1])
        i = next(i for i,r in enumerate(rows) if r[1:] == reverse)
        successor[h] = rows[(i-1)%len(rows)][1:]
    seen = set(); result = []
    for first in sorted(successor):
        if first in seen: continue
        walk = []; h = first
        while h not in seen:
            seen.add(h); walk.append(h); h = successor[h]
        # Remove dangling out-and-back excursions without hiding source edges.
        changed = True
        while changed and len(walk)>1:
            changed = False
            for i,a in enumerate(walk):
                b = walk[(i+1)%len(walk)]
                if a[0] == b[0] and a[1] != b[1]:
                    walk = [x for j,x in enumerate(walk) if j not in (i,(i+1)%len(walk))]
                    changed = True; break
        if walk and len(set(g for g,d in walk)) == len(walk): result.append(walk)
    return result


def junction_signature(endpoints):
    """Canonical labeled incidence, independent of native vertex enumeration.

    Labels identify verified parent/parameter-ranked fragments. Endpoint numbers
    refer to increasing native curve parameter, not transient topology order.
    """
    junctions = {}
    for fragment, pair in endpoints.items():
        if len(pair) != 2:
            raise ValueError('A fragment needs two parameter endpoints')
        for end, vertex in enumerate(pair):
            junctions.setdefault(vertex, []).append((fragment, end))
    return sorted(sorted(rows) for rows in junctions.values())
