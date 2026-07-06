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
# Usage: make-fixtures.py [path-to-draco_encoder]
import json
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
(SCENE_DIR / "scene.json").write_bytes(
    json.dumps({"scene": {"base": "0,0", "parcels": ["0,0", "1,0"]}},
               separators=(",", ":")).encode()
)
wrote.append("scene-lod/model.glb")
wrote.append("scene-lod/scene.json")

if len(sys.argv) > 1 and sys.argv[1]:
    emit("draco-quad.glb", draco_quad(sys.argv[1]))
elif (OUT / "draco-quad.glb").exists():
    print("WARNING: no draco_encoder given, keeping the committed draco-quad.glb")
else:
    print("WARNING: no draco_encoder given and no committed draco-quad.glb")

print("wrote", " ".join(wrote))
