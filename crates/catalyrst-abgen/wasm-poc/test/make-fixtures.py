#!/usr/bin/env python3
# Generates the wasm-lab test fixtures into wasm-poc/test/fixtures/:
#   jpeg-quad.glb       quad with a JPEG baseColorTexture (libjpeg lane)
#   normal-quad.glb     PNG baseColor + PNG normal map + authored tangents
#                       (crunched-DXT5 normal lane; no-transcendental control)
#   draco-quad.glb      positions-only quad compressed with draco_encoder
#                       (KHR_draco_mesh_compression lane)
#   gamma-quad.glb      PNG baseColor with a nontrivial gAMA chunk whose pixels
#                       sweep all 256 byte values (png-gamma powf LUT lane)
#   transform-quad.glb  KHR_texture_transform rotation (sin/cos lane)
#   tangent-quad.glb    normal map with the TANGENT accessor omitted on a
#                       skewed quad (tangent-generation acos lane)
#   multimat-quad.glb   two primitives/materials: JPEG + rotation on mat0,
#                       gAMA PNG + normal map without tangents on mat1
#   scene-lod/          scene.json (base 0,0, parcels 0,0;1,0) + model.glb
#                       with an opaque JPEG material and a BLEND RGBA PNG
#                       material (scene entity + LOD atlas lanes)
#   dense-decimate-lod/ 33x33 sine-displaced textured grid (2048 tris) on the
#                       two-parcel scene (meshopt decimation cross-target lane)
#   crop-overhang-lod/  textured ground slab overhanging the parcel union on
#                       both x sides, segmented so tris straddle the crop
#                       planes and one quad falls fully outside (crop lane)
#   placements-iss-lod/ tri.glb + cube.glb + scene.json + an InitialSceneState
#                       descriptor placing them by content hash, including a
#                       duplicate-GLB TRS and a negative-scale mirror
#                       (ISS placements assemble lane)
#   badjpeg-pair/       good.glb (jpeg-quad recipe) + bad.glb whose embedded
#                       JPEG has the SOF0 precision byte corrupted (per-image
#                       decode recovery lane)
# Usage: make-fixtures.py [path-to-draco_encoder]
import hashlib
import json
import math
import struct
import subprocess
import sys
import tempfile
import zlib
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUT = HERE / "fixtures"
OUT.mkdir(exist_ok=True)
JPEG = (HERE / "../../testdata/gradient-16x16.jpg").resolve().read_bytes()


def png_rgba(width, height, pixel_fn, gama=None):
    raw = b""
    for y in range(height):
        raw += b"\x00"
        for x in range(width):
            raw += bytes(pixel_fn(x, y))
    def chunk(typ, data):
        c = typ + data
        return struct.pack(">I", len(data)) + c + struct.pack(">I", zlib.crc32(c))
    out = b"\x89PNG\r\n\x1a\n" + chunk(
        b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    )
    if gama is not None:
        out += chunk(b"gAMA", struct.pack(">I", gama))
    return out + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def pad4(b, fill):
    return b + fill * ((4 - len(b) % 4) % 4)


def glb(gltf_json, bin_blob):
    js = pad4(json.dumps(gltf_json, separators=(",", ":")).encode(), b" ")
    bb = pad4(bin_blob, b"\x00")
    total = 12 + 8 + len(js) + 8 + len(bb)
    return (
        struct.pack("<III", 0x46546C67, 2, total)
        + struct.pack("<II", len(js), 0x4E4F534A) + js
        + struct.pack("<II", len(bb), 0x004E4942) + bb
    )


POS = [(-0.5, -0.5, 0.0), (0.5, -0.5, 0.0), (-0.5, 0.5, 0.0), (0.5, 0.5, 0.0)]
NRM = [(0.0, 0.0, 1.0)] * 4
TAN = [(1.0, 0.0, 0.0, 1.0)] * 4
UV = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
IDX = [0, 1, 2, 2, 1, 3]

# Skewed fourth vertex: edge dot products become irrational so the tangent
# acos actually sees rounding-sensitive inputs (a flat axis-aligned quad only
# feeds it 0/±0.707/1-class values).
SKEW_POS = [(-0.5, -0.5, 0.0), (0.5, -0.5, 0.0), (-0.5, 0.5, 0.0), (0.55, 0.4, 0.2)]

KTT = {"rotation": 0.35, "offset": [0.1, 0.2], "scale": [1.5, 0.75]}


def fvecs(vs):
    return b"".join(struct.pack("<%df" % len(v), *v) for v in vs)


def minmax(vs):
    n = len(vs[0])
    return (
        [min(v[i] for v in vs) for i in range(n)],
        [max(v[i] for v in vs) for i in range(n)],
    )


class Builder:
    def __init__(self):
        self.blob = b""
        self.views = []
        self.accessors = []

    def add(self, data, target=None):
        off = len(self.blob)
        self.blob += data
        self.blob = pad4(self.blob, b"\x00")
        v = {"buffer": 0, "byteOffset": off, "byteLength": len(data)}
        if target:
            v["target"] = target
        self.views.append(v)
        return len(self.views) - 1

    def acc(self, view, ctype, count, atype, mn=None, mx=None):
        a = {"bufferView": view, "componentType": ctype, "count": count, "type": atype}
        if mn is not None:
            a["min"], a["max"] = mn, mx
        self.accessors.append(a)
        return len(self.accessors) - 1

    def quad_prim(self, positions, material, tangents):
        mn, mx = minmax(positions)
        attrs = {
            "POSITION": self.acc(self.add(fvecs(positions), 34962), 5126, 4, "VEC3", mn, mx),
            "NORMAL": self.acc(self.add(fvecs(NRM), 34962), 5126, 4, "VEC3"),
        }
        if tangents:
            attrs["TANGENT"] = self.acc(self.add(fvecs(TAN), 34962), 5126, 4, "VEC4")
        attrs["TEXCOORD_0"] = self.acc(self.add(fvecs(UV), 34962), 5126, 4, "VEC2")
        a_idx = self.acc(self.add(struct.pack("<6H", *IDX), 34963), 5123, 6, "SCALAR")
        return {"attributes": attrs, "indices": a_idx, "material": material}

    def finish(self, gltf_json):
        gltf_json["buffers"] = [{"byteLength": len(pad4(self.blob, b"\x00"))}]
        gltf_json["bufferViews"] = self.views
        gltf_json["accessors"] = self.accessors
        return glb(gltf_json, self.blob)


def gltf_head(extensions_used):
    head = {"asset": {"version": "2.0", "generator": "abgen wasm-lab fixtures"}}
    if extensions_used:
        head["extensionsUsed"] = extensions_used
    return head


def textured_quad(images, use_normal_map, positions=POS, omit_tangents=False,
                  base_color_ext=None, extensions_used=None):
    b = Builder()
    prim = b.quad_prim(positions, 0, not omit_tangents)

    imgs = [{"bufferView": b.add(data), "mimeType": mime} for mime, data in images]

    base_color = {"index": 0}
    if base_color_ext:
        base_color["extensions"] = base_color_ext
    material = {
        "name": "mat",
        "pbrMetallicRoughness": {
            "baseColorTexture": base_color,
            "metallicFactor": 0.0,
            "roughnessFactor": 1.0,
        },
    }
    if use_normal_map:
        material["normalTexture"] = {"index": 1}

    gltf_json = gltf_head(extensions_used)
    gltf_json.update({
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0, "name": "quad"}],
        "meshes": [{"name": "quad", "primitives": [prim]}],
        "materials": [material],
        "textures": [{"source": i, "sampler": 0} for i in range(len(imgs))],
        "samplers": [{"magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497}],
        "images": imgs,
    })
    return b.finish(gltf_json)


def multi_quad(prims, materials, images, extensions_used=None):
    b = Builder()
    prim_jsons = [
        b.quad_prim(spec["positions"], spec["material"], spec["tangents"])
        for spec in prims
    ]

    imgs = [{"bufferView": b.add(data), "mimeType": mime} for mime, data in images]

    gltf_json = gltf_head(extensions_used)
    gltf_json.update({
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0, "name": "quads"}],
        "meshes": [{"name": "quads", "primitives": prim_jsons}],
        "materials": materials,
        "textures": [{"source": i, "sampler": 0} for i in range(len(imgs))],
        "samplers": [{"magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497}],
        "images": imgs,
    })
    return b.finish(gltf_json)


def draco_quad(encoder):
    with tempfile.TemporaryDirectory() as td:
        obj = Path(td) / "quad.obj"
        drc = Path(td) / "quad.drc"
        obj.write_text(
            "".join(f"v {x} {y} {z}\n" for x, y, z in POS) + "f 1 2 3\nf 3 2 4\n"
        )
        subprocess.run(
            [encoder, "-i", str(obj), "-o", str(drc), "-qp", "14", "-cl", "7"],
            check=True, capture_output=True,
        )
        payload = drc.read_bytes()

    blob = pad4(payload, b"\x00")
    gltf_json = {
        "asset": {"version": "2.0", "generator": "abgen wasm-lab fixtures"},
        "extensionsUsed": ["KHR_draco_mesh_compression"],
        "extensionsRequired": ["KHR_draco_mesh_compression"],
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0, "name": "dracoquad"}],
        "meshes": [{
            "name": "dracoquad",
            "primitives": [{
                "attributes": {"POSITION": 0},
                "indices": 1,
                "material": 0,
                "extensions": {
                    "KHR_draco_mesh_compression": {
                        "bufferView": 0,
                        "attributes": {"POSITION": 0},
                    }
                },
            }],
        }],
        "materials": [{
            "name": "mat",
            "pbrMetallicRoughness": {
                "baseColorFactor": [0.8, 0.3, 0.1, 1.0],
                "metallicFactor": 0.0,
                "roughnessFactor": 1.0,
            },
        }],
        "buffers": [{"byteLength": len(blob)}],
        "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": len(payload)}],
        "accessors": [
            {"componentType": 5126, "count": 4, "type": "VEC3",
             "min": [-0.5, -0.5, 0.0], "max": [0.5, 0.5, 0.0]},
            {"componentType": 5123, "count": 6, "type": "SCALAR"},
        ],
    }
    return glb(gltf_json, b"" if not blob else blob)


base_png = png_rgba(16, 16, lambda x, y: (180, 90 + 4 * y, 60 + 4 * x, 255))
normal_png = png_rgba(16, 16, lambda x, y: (118 + x, 122 + y, 250, 255))
# gAMA 100000 = gamma 1.0 -> LUT exponent 1/2.2, mid 128 -> ~187: passes the
# nontriviality gate (the standard 45455 would be rejected as a no-op); the
# r=g=b sweep hits every one of the 256 powf-built LUT slots.
gamma_png = png_rgba(16, 16, lambda x, y: (x + 16 * y,) * 3 + (255,), gama=100000)
alpha_png = png_rgba(16, 16, lambda x, y: (200, 40 + 4 * y, 30 + 4 * x, 64 + 8 * x))

wrote = []


def emit(name, data):
    (OUT / name).write_bytes(data)
    wrote.append(name)


emit("jpeg-quad.glb", textured_quad([("image/jpeg", JPEG)], False))
emit("normal-quad.glb",
     textured_quad([("image/png", base_png), ("image/png", normal_png)], True))
emit("gamma-quad.glb", textured_quad([("image/png", gamma_png)], False))
emit("transform-quad.glb",
     textured_quad([("image/png", base_png)], False,
                   base_color_ext={"KHR_texture_transform": KTT},
                   extensions_used=["KHR_texture_transform"]))
emit("tangent-quad.glb",
     textured_quad([("image/png", base_png), ("image/png", normal_png)], True,
                   positions=SKEW_POS, omit_tangents=True))

MM_POS2 = [(1.0, -0.5, 0.0), (2.0, -0.5, 0.0), (1.0, 0.5, 0.0), (2.05, 0.4, 0.2)]
emit("multimat-quad.glb", multi_quad(
    [
        {"positions": POS, "material": 0, "tangents": True},
        {"positions": MM_POS2, "material": 1, "tangents": False},
    ],
    [
        {
            "name": "mat0",
            "pbrMetallicRoughness": {
                "baseColorTexture": {"index": 0, "extensions": {"KHR_texture_transform": KTT}},
                "metallicFactor": 0.0,
                "roughnessFactor": 1.0,
            },
        },
        {
            "name": "mat1",
            "pbrMetallicRoughness": {
                "baseColorTexture": {"index": 1},
                "metallicFactor": 0.0,
                "roughnessFactor": 1.0,
            },
            "normalTexture": {"index": 2},
        },
    ],
    [("image/jpeg", JPEG), ("image/png", gamma_png), ("image/png", normal_png)],
    extensions_used=["KHR_texture_transform"],
))

# glTF is right-handed, Unity x = -x: parcel (0,0) is RH x in [-16,0],
# parcel (1,0) is RH x in [-32,-16], both z in [0,16].
LOD_POS_A = [(-8.5, 0.0, 8.0), (-7.5, 0.0, 8.0), (-8.5, 1.0, 8.0), (-7.5, 1.0, 8.0)]
LOD_POS_B = [(-24.5, 0.0, 8.0), (-23.5, 0.0, 8.0), (-24.5, 1.0, 8.0), (-23.5, 1.0, 8.0)]
SCENE_DIR = OUT / "scene-lod"
SCENE_DIR.mkdir(exist_ok=True)
(SCENE_DIR / "model.glb").write_bytes(multi_quad(
    [
        {"positions": LOD_POS_A, "material": 0, "tangents": True},
        {"positions": LOD_POS_B, "material": 1, "tangents": True},
    ],
    [
        {
            "name": "smat0",
            "pbrMetallicRoughness": {
                "baseColorTexture": {"index": 0},
                "metallicFactor": 0.0,
                "roughnessFactor": 1.0,
            },
        },
        {
            "name": "smat1",
            "pbrMetallicRoughness": {
                "baseColorTexture": {"index": 1},
                "metallicFactor": 0.0,
                "roughnessFactor": 1.0,
            },
            "alphaMode": "BLEND",
        },
    ],
    [("image/jpeg", JPEG), ("image/png", alpha_png)],
))
SCENE_JSON = json.dumps({"scene": {"base": "0,0", "parcels": ["0,0", "1,0"]}},
                        separators=(",", ":")).encode()
(SCENE_DIR / "scene.json").write_bytes(SCENE_JSON)
wrote.append("scene-lod/model.glb")
wrote.append("scene-lod/scene.json")


def mesh_glb(prims, materials, images):
    b = Builder()
    prim_jsons = []
    for spec in prims:
        positions = spec["positions"]
        mn, mx = minmax(positions)
        attrs = {
            "POSITION": b.acc(b.add(fvecs(positions), 34962), 5126,
                              len(positions), "VEC3", mn, mx),
            "NORMAL": b.acc(b.add(fvecs(spec["normals"]), 34962), 5126,
                            len(positions), "VEC3"),
            "TEXCOORD_0": b.acc(b.add(fvecs(spec["uvs"]), 34962), 5126,
                                len(positions), "VEC2"),
        }
        idx = spec["indices"]
        a_idx = b.acc(b.add(struct.pack("<%dH" % len(idx), *idx), 34963),
                      5123, len(idx), "SCALAR")
        prim_jsons.append(
            {"attributes": attrs, "indices": a_idx, "material": spec["material"]})
    imgs = [{"bufferView": b.add(data), "mimeType": mime} for mime, data in images]
    gltf_json = gltf_head(None)
    gltf_json.update({
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0, "name": "mesh"}],
        "meshes": [{"name": "mesh", "primitives": prim_jsons}],
        "materials": materials,
        "textures": [{"source": i, "sampler": 0} for i in range(len(imgs))],
        "samplers": [{"magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497}],
        "images": imgs,
    })
    return b.finish(gltf_json)


def tex_mat(name, index):
    return {
        "name": name,
        "pbrMetallicRoughness": {
            "baseColorTexture": {"index": index},
            "metallicFactor": 0.0,
            "roughnessFactor": 1.0,
        },
    }


def grid_mesh(n):
    positions, normals, uvs = [], [], []
    for j in range(n + 1):
        for i in range(n + 1):
            x = i / n
            z = j / n
            y = 0.05 * (math.sin(x * 12) + math.cos(z * 12)) \
                * (1 + 0.3 * math.sin(x * 5 + z * 7))
            positions.append((x * 10, y, z * 10))
            normals.append((0.0, 1.0, 0.0))
            uvs.append((x, z))
    indices = []
    for j in range(n):
        for i in range(n):
            a = j * (n + 1) + i
            bq = a + 1
            c = a + n + 1
            d = c + 1
            indices += [a, c, bq, bq, c, d]
    return positions, normals, uvs, indices


DENSE_DIR = OUT / "dense-decimate-lod"
DENSE_DIR.mkdir(exist_ok=True)
gp, gn, gu, gi = grid_mesh(32)
grid_png = png_rgba(64, 64, lambda x, y: (x * 4 % 256, y * 4 % 256, (x + y) * 2 % 256, 255))
(DENSE_DIR / "model.glb").write_bytes(mesh_glb(
    [{"positions": gp, "normals": gn, "uvs": gu, "indices": gi, "material": 0}],
    [tex_mat("gridmat", 0)],
    [("image/png", grid_png)],
))
(DENSE_DIR / "scene.json").write_bytes(SCENE_JSON)
wrote.append("dense-decimate-lod/model.glb")
wrote.append("dense-decimate-lod/scene.json")

# Parcel union in RH space is x [-32.05, 0.05], z [-0.05, 16.05]; segment
# edges at -28/-16/-4 keep interior quads intact while [-40,-28] and [-4,8]
# straddle a crop plane and [20,28] falls fully outside.
SLAB_SEGMENTS = [(-40.0, -28.0), (-28.0, -16.0), (-16.0, -4.0), (-4.0, 8.0), (20.0, 28.0)]
sp, sn, su, si = [], [], [], []
for x0, x1 in SLAB_SEGMENTS:
    base = len(sp)
    sp += [(x0, 0.0, 4.0), (x1, 0.0, 4.0), (x0, 0.0, 12.0), (x1, 0.0, 12.0)]
    sn += [(0.0, 1.0, 0.0)] * 4
    su += [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
    si += [base, base + 1, base + 2, base + 2, base + 1, base + 3]
CROP_DIR = OUT / "crop-overhang-lod"
CROP_DIR.mkdir(exist_ok=True)
(CROP_DIR / "model.glb").write_bytes(mesh_glb(
    [{"positions": sp, "normals": sn, "uvs": su, "indices": si, "material": 0}],
    [tex_mat("slabmat", 0)],
    [("image/png", base_png)],
))
(CROP_DIR / "scene.json").write_bytes(SCENE_JSON)
wrote.append("crop-overhang-lod/model.glb")
wrote.append("crop-overhang-lod/scene.json")


def cube_mesh():
    axes = [
        ((0.0, 1.0, 0.0), (0.0, 0.0, 1.0)),
        ((0.0, 0.0, 1.0), (0.0, 1.0, 0.0)),
        ((0.0, 0.0, 1.0), (1.0, 0.0, 0.0)),
        ((1.0, 0.0, 0.0), (0.0, 0.0, 1.0)),
        ((1.0, 0.0, 0.0), (0.0, 1.0, 0.0)),
        ((0.0, 1.0, 0.0), (1.0, 0.0, 0.0)),
    ]
    positions, normals, uvs, indices = [], [], [], []
    for u, v in axes:
        n = (u[1] * v[2] - u[2] * v[1],
             u[2] * v[0] - u[0] * v[2],
             u[0] * v[1] - u[1] * v[0])
        base = len(positions)
        for su_, sv_ in [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)]:
            positions.append((n[0] * 0.5 + u[0] * su_ + v[0] * sv_,
                              n[1] * 0.5 + u[1] * su_ + v[1] * sv_,
                              n[2] * 0.5 + u[2] * su_ + v[2] * sv_))
            normals.append(n)
            uvs.append((su_ + 0.5, sv_ + 0.5))
        indices += [base, base + 1, base + 2, base, base + 2, base + 3]
    return positions, normals, uvs, indices


PLACE_DIR = OUT / "placements-iss-lod"
PLACE_DIR.mkdir(exist_ok=True)
tri_png = png_rgba(16, 16, lambda x, y: (240, 40 + 4 * y, 40 + 4 * x, 255))
cube_png = png_rgba(16, 16, lambda x, y: (40 + 4 * x, 240, 40 + 4 * y, 255))
tri_bytes = mesh_glb(
    [{"positions": [(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)],
      "normals": [(0.0, 0.0, 1.0)] * 3,
      "uvs": [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)],
      "indices": [0, 1, 2], "material": 0}],
    [tex_mat("trimat", 0)],
    [("image/png", tri_png)],
)
cp, cn, cu, ci = cube_mesh()
cube_bytes = mesh_glb(
    [{"positions": cp, "normals": cn, "uvs": cu, "indices": ci, "material": 0}],
    [tex_mat("cubemat", 0)],
    [("image/png", cube_png)],
)
(PLACE_DIR / "tri.glb").write_bytes(tri_bytes)
(PLACE_DIR / "cube.glb").write_bytes(cube_bytes)
(PLACE_DIR / "scene.json").write_bytes(SCENE_JSON)


def iss_asset(h, pos, rot=None, scale=None):
    a = {"hash": h, "position": {"x": pos[0], "y": pos[1], "z": pos[2]}}
    if rot is not None:
        a["rotation"] = {"x": rot[0], "y": rot[1], "z": rot[2], "w": rot[3]}
    if scale is not None:
        a["scale"] = {"x": scale[0], "y": scale[1], "z": scale[2]}
    return a


tri_hash = hashlib.sha256(tri_bytes).hexdigest()
cube_hash = hashlib.sha256(cube_bytes).hexdigest()
iss_doc = {
    "version": 1,
    "sceneId": "fixture",
    "assets": [
        iss_asset(tri_hash, (1.0, 2.0, 3.0)),
        iss_asset(cube_hash, (5.0, 0.5, 5.0),
                  (0.0, 0.7071065902709961, 0.0, 0.7071070671081543),
                  (0.97, 0.97, 0.97)),
        iss_asset(cube_hash, (10.0, 0.0, 8.0), None, (-1.0, 1.0, 1.0)),
    ],
}
(PLACE_DIR / "fixture_InitialSceneState.json").write_bytes(
    json.dumps(iss_doc, separators=(",", ":")).encode())
wrote.append("placements-iss-lod/tri.glb")
wrote.append("placements-iss-lod/cube.glb")
wrote.append("placements-iss-lod/scene.json")
wrote.append("placements-iss-lod/fixture_InitialSceneState.json")

BAD_DIR = OUT / "badjpeg-pair"
BAD_DIR.mkdir(exist_ok=True)
(BAD_DIR / "good.glb").write_bytes(textured_quad([("image/jpeg", JPEG)], False))
# SOF0 precision 2 forces a libjpeg ERREXIT (hard error path); a truncated
# tail would only warn (jdatasrc fakes an EOI) and test nothing.
sof = JPEG.find(b"\xff\xc0")
assert sof >= 0, "fixture JPEG has no SOF0 marker"
bad_jpeg = bytearray(JPEG)
bad_jpeg[sof + 4] = 2
(BAD_DIR / "bad.glb").write_bytes(
    textured_quad([("image/jpeg", bytes(bad_jpeg))], False))
wrote.append("badjpeg-pair/good.glb")
wrote.append("badjpeg-pair/bad.glb")

if len(sys.argv) > 1 and sys.argv[1]:
    emit("draco-quad.glb", draco_quad(sys.argv[1]))
elif (OUT / "draco-quad.glb").exists():
    print("WARNING: no draco_encoder given, keeping the committed draco-quad.glb")
else:
    print("WARNING: no draco_encoder given and no committed draco-quad.glb")

print("wrote", " ".join(wrote))
