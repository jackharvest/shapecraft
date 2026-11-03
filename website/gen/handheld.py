"""A clamshell handheld console, modeled step by step with ShapeCraft's own commands.

Used by gifs.py for the site's speed-lapse, and runnable on its own as a check:

  python3 website/gen/handheld.py OUT.jsonl && shapecraft-cli run OUT.jsonl --png out.png

Each step is (command, params). Item ids are the ones the commands hand out in this order.
Millimetres; the lower half lies on XY with its hinge along X at the back (y = 80).
"""
import json
import sys

COBALT = {"name": "Cobalt", "color": [0.10, 0.23, 0.78], "metal": 0.15, "gloss": 0.55}
GRAPHITE = {"name": "Graphite", "color": [0.13, 0.13, 0.15], "metal": 0.0, "gloss": 0.35}
GLASS = {"name": "Screen", "color": [0.02, 0.02, 0.03], "metal": 0.0, "gloss": 0.95}

LID_OPEN = 32
LID = [6, 9]  # lid body and its glass
NOT_IN_HISTORY = {"select", "deselect_all", "exit_sketching", "rectangle", "circle", "sketch_fillet", "text"}
TEXT_REGIONS = list(range(10))  # degrees past flat that the lid ends up at (a 148° opening)


def plane_at(z):
    return {"origin": [0, 0, z], "normal": [0, 0, 1]}


# A sketch on plane_at(z) has u along -Y and v along +X, so world (x, y) is (-y, x) there.
def P(x, y):
    return [-y, x]


def S(w, h):
    return [h, w]


def steps():
    s = []
    a = s.append
    # 1 · lower half
    a(("sketch", {"plane": "xy"}))                                        # sketch 1
    a(("rectangle", {"center": [0, 40], "size": [84, 80]}))
    a(("sketch_fillet", {"radius": 8}))
    a(("exit_sketching", {}))
    a(("extrude", {"sketch": 1, "region": 0, "distance": 11}))            # body 2
    a(("select", {"picks": [{"kind": "body", "body": 2}]}))
    a(("fillet", {"radius": 2}))
    # 2 · hinge
    a(("sketch", {"origin": [-31, 0, 0], "normal": [1, 0, 0]}))           # sketch 3
    a(("circle", {"center": [81, 11], "radius": 6}))
    a(("exit_sketching", {}))
    a(("extrude", {"sketch": 3, "region": 0, "distance": 62, "result": "new"}))  # body 4
    # 3 · lid, lying flat for now
    a(("sketch", {"plane": "xy"}))                                        # sketch 5
    a(("rectangle", {"center": [0, 122], "size": [84, 80]}))
    a(("sketch_fillet", {"radius": 8}))
    a(("exit_sketching", {}))
    a(("extrude", {"sketch": 5, "region": 0, "distance": 10, "result": "new"}))  # body 6
    a(("select", {"picks": [{"kind": "body", "body": 6}]}))
    a(("fillet", {"radius": 2}))
    # 4 · screen bezel and glass
    a(("sketch", plane_at(10)))                                           # sketch 7
    a(("rectangle", {"center": P(0, 127), "size": S(68, 50)}))
    a(("sketch_fillet", {"radius": 4}))
    a(("exit_sketching", {}))
    a(("extrude", {"sketch": 7, "region": 0, "distance": -1.2, "result": "subtract"}))
    a(("sketch", plane_at(8.8)))                                          # sketch 8
    a(("rectangle", {"center": P(0, 128), "size": S(58, 39)}))
    a(("exit_sketching", {}))
    a(("extrude", {"sketch": 8, "region": 0, "distance": 0.6, "result": "new"}))  # body 9
    # 5 · d-pad well and cross
    a(("sketch", plane_at(11)))                                           # sketch 10
    a(("circle", {"center": P(-24, 40), "radius": 11.5}))
    a(("exit_sketching", {}))
    a(("extrude", {"sketch": 10, "region": 0, "distance": -1.2, "result": "subtract"}))
    a(("sketch", plane_at(9.8)))                                          # sketch 11
    a(("rectangle", {"center": P(-24, 40), "size": S(19, 6.4)}))
    a(("rectangle", {"center": P(-24, 40), "size": S(6.4, 19)}))
    a(("exit_sketching", {}))
    a(("extrude", {"sketch": 11, "regions": [0, 1, 2, 3, 4], "distance": 3.2, "result": "new"}))  # body 12
    # 6 · A and B
    a(("sketch", plane_at(10)))                                           # sketch 13
    a(("circle", {"center": P(19, 34), "radius": 4.8}))
    a(("circle", {"center": P(31, 43), "radius": 4.8}))
    a(("exit_sketching", {}))
    a(("extrude", {"sketch": 13, "regions": [0, 1], "distance": 3.2, "result": "new"}))  # body 14 (+15)
    # 7 · start, select, speaker
    a(("sketch", plane_at(10.2)))                                         # sketch
    a(("circle", {"center": P(-29, 15), "radius": 2.2}))
    a(("circle", {"center": P(-19, 15), "radius": 2.2}))
    a(("exit_sketching", {}))
    a(("extrude", {"sketch": 15, "regions": [0, 1], "distance": 1.6, "result": "new"}))  # body 16
    a(("sketch", plane_at(11)))                                           # sketch 17
    holes = [[21 + 4.2 * i, 9 + 4.2 * j] for j in range(3) for i in range(4)]
    for h in holes:
        a(("circle", {"center": P(*h), "radius": 0.9}))
    a(("exit_sketching", {}))
    a(("extrude", {"sketch": 17, "regions": list(range(len(holes))), "distance": -1.5, "result": "subtract"}))
    # 8 · shoulder buttons at the back corners
    a(("sketch", plane_at(3)))                                            # sketch 18
    a(("rectangle", {"corner": P(32.5, 70), "opposite": P(41.4, 81.5)}))
    a(("rectangle", {"corner": P(-41.4, 70), "opposite": P(-32.5, 81.5)}))
    a(("exit_sketching", {}))
    a(("extrude", {"sketch": 18, "regions": [0, 1], "distance": 6, "result": "new"}))  # body 19
    # 9 · lettering under the screen
    a(("sketch", plane_at(10)))                                           # sketch 20
    a(("text", {"at": P(-14, 90.5), "text": "SHAPECRAFT", "size": 4.2, "angle": 90}))
    a(("exit_sketching", {}))
    a(("extrude", {"sketch": 20, "regions": TEXT_REGIONS, "distance": -0.5, "result": "subtract"}))
    # 10 · colour
    a(("set_appearance", {"items": [2, 4, 6], "appearance": COBALT}))
    a(("set_appearance", {"items": [12, 14, 16, 19], "appearance": GRAPHITE}))
    a(("set_appearance", {"items": [9], "appearance": GLASS}))
    a(("deselect_all", {}))
    # 11 · open the lid (the speed-lapse eases this angle in with edit_step)
    a(("rotate", {"origin": [0, 81, 11], "axis": [1, 0, 0], "angle": LID_OPEN, "items": LID}))
    return s


def write(path, items):
    with open(path, "w") as f:
        for c, p in items:
            f.write(json.dumps({"command": c, "params": p}) + "\n")


if __name__ == "__main__":
    write(sys.argv[1], steps())
