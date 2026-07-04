#pragma once
#include "rust/cxx.h"
#include <cstdint>
#include <vector>
#include <memory>

// Forward declarations - defined in ffi.rs.h
struct MeshAttribute;
struct MeshConfig;
struct PositionQuantBounds;

// Forward declaration for draco::Mesh
namespace draco {
class Mesh;
}

// DracoMesh class - wraps draco::Mesh
class DracoMesh {
public:
  std::unique_ptr<draco::Mesh> mesh;

  explicit DracoMesh(std::unique_ptr<draco::Mesh> m);
  ~DracoMesh();
};


rust::Vec<uint8_t> decode_point_cloud(rust::Slice<const uint8_t> data);

// Cache API - returns opaque type
std::unique_ptr<DracoMesh> create_mesh(rust::Slice<const uint8_t> data);

// Mesh Config from DracoMesh
bool compute_mesh_config(const DracoMesh &mesh, MeshConfig &config);

// Decode to pre-allocated buffer
size_t decode_mesh_to_buffer(const DracoMesh &mesh, uint8_t *out_ptr, size_t out_len);

// Re-decodes the input buffer with SetSkipAttributeTransform(POSITION) to
// recover the AttributeQuantizationTransform parameters for the POSITION
// attribute. Unity's draco importer writes `m_LocalAABB` using the
// pre-quantization bounds carried in the draco header, not the post-decode
// stream — so populating these into the synthesized POSITION accessor's
// min/max removes a ~1.5e-4 / 1 ULP drift on the largest-magnitude axis.
//
// Returns `valid = true` only when the POSITION attribute used
// AttributeQuantizationTransform (i.e. quantization_bits >= 1). For un-
// quantized inputs (rare in GLTF/Draco bitstreams; would carry full float32
// positions verbatim), the post-decode stream scan is already exact and no
// override is needed.
bool decode_position_quant_bounds(rust::Slice<const uint8_t> data,
                                  PositionQuantBounds &out);
