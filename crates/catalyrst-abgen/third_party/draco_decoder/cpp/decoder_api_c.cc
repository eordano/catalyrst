// decoder_api_c.cc — plain-C ABI twin of decoder_api.cc for targets where
// the cxx bridge is unavailable (wasm32). Mirrors its layout contract
// exactly: [indices (u16 when index_count <= 65535, else u32)] then each
// attribute's ConvertValue'd data, attributes sorted by unique_id.

#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <algorithm>
#include <limits>
#include <memory>
#include <vector>

#include "draco/attributes/attribute_quantization_transform.h"
#include "draco/attributes/attribute_transform_type.h"
#include "draco/attributes/geometry_attribute.h"
#include "draco/attributes/point_attribute.h"
#include "draco/compression/decode.h"
#include "draco/core/decoder_buffer.h"
#include "draco/mesh/mesh.h"

typedef struct {
  uint32_t dim;
  int32_t data_type;
  uint32_t offset;
  uint32_t length;
  uint32_t unique_id;
} draco_c_attr;

static size_t sizeof_data_type(draco::DataType type) {
  switch (type) {
  case draco::DT_INT8:
  case draco::DT_UINT8:
    return 1;
  case draco::DT_INT16:
  case draco::DT_UINT16:
    return 2;
  case draco::DT_INT32:
  case draco::DT_UINT32:
  case draco::DT_FLOAT32:
    return 4;
  case draco::DT_INT64:
  case draco::DT_UINT64:
  case draco::DT_FLOAT64:
    return 8;
  default:
    return 0;
  }
}

struct SortedAttrs {
  std::vector<const draco::PointAttribute *> attrs;
};

static SortedAttrs sorted_attrs(const draco::Mesh &mesh) {
  SortedAttrs out;
  out.attrs.reserve(mesh.num_attributes());
  for (int i = 0; i < mesh.num_attributes(); ++i) {
    out.attrs.push_back(mesh.attribute(i));
  }
  std::sort(out.attrs.begin(), out.attrs.end(),
            [](const draco::PointAttribute *a, const draco::PointAttribute *b) {
              return a->unique_id() < b->unique_id();
            });
  return out;
}

template <typename T>
static bool write_attr(const draco::PointAttribute *attr, int dim,
                       uint32_t num_points, uint8_t *&out, uint8_t *out_end) {
  for (draco::PointIndex j(0); j < num_points; ++j) {
    T v[4] = {};
    attr->ConvertValue(attr->mapped_index(j), v);
    const size_t n = sizeof(T) * static_cast<size_t>(dim);
    if (out + n > out_end) {
      return false;
    }
    memcpy(out, v, n);
    out += n;
  }
  return true;
}

extern "C" int32_t draco_c_decode_mesh(const uint8_t *data, size_t len,
                            uint32_t *out_vertex_count,
                            uint32_t *out_index_count, size_t *out_buffer_size,
                            draco_c_attr *attrs_out, uint32_t attrs_cap,
                            uint32_t *out_attr_count, uint8_t **out_blob,
                            size_t *out_written) {
  if (!data || !out_vertex_count || !out_index_count || !out_buffer_size ||
      !attrs_out || !out_attr_count || !out_blob || !out_written) {
    return 1;
  }

  draco::DecoderBuffer buffer;
  buffer.Init(reinterpret_cast<const char *>(data), len);
  draco::Decoder decoder;
  auto status_or_geometry = decoder.DecodeMeshFromBuffer(&buffer);
  if (!status_or_geometry.ok()) {
    return 2;
  }
  std::unique_ptr<draco::Mesh> mesh = std::move(status_or_geometry).value();
  if (!mesh) {
    return 2;
  }

  const uint32_t vertex_count = mesh->num_points();
  const uint32_t index_count = mesh->num_faces() * 3;
  const uint32_t index_length =
      index_count <= std::numeric_limits<uint16_t>::max()
          ? index_count * static_cast<uint32_t>(sizeof(uint16_t))
          : index_count * static_cast<uint32_t>(sizeof(uint32_t));

  SortedAttrs sorted = sorted_attrs(*mesh);
  if (sorted.attrs.size() > attrs_cap) {
    return 3;
  }

  uint32_t current_offset = index_length;
  uint32_t attr_count = 0;
  for (const draco::PointAttribute *attr : sorted.attrs) {
    draco_c_attr &a = attrs_out[attr_count++];
    a.dim = attr->num_components();
    a.data_type = static_cast<int32_t>(attr->data_type());
    a.unique_id = attr->unique_id();
    a.offset = current_offset;
    a.length = a.dim * vertex_count *
               static_cast<uint32_t>(sizeof_data_type(attr->data_type()));
    current_offset += a.length;
  }
  const size_t buffer_size = current_offset;

  uint8_t *blob = static_cast<uint8_t *>(malloc(buffer_size ? buffer_size : 1));
  if (!blob) {
    return 4;
  }
  uint8_t *out = blob;
  uint8_t *out_end = blob + buffer_size;

  const int num_faces = mesh->num_faces();
  const bool use_u16 =
      index_count <= std::numeric_limits<uint16_t>::max();
  for (draco::FaceIndex i(0); i < num_faces; ++i) {
    const auto &face = mesh->face(i);
    for (int j = 0; j < 3; ++j) {
      if (use_u16) {
        const uint16_t val = static_cast<uint16_t>(face[j].value());
        if (out + sizeof(val) > out_end) { free(blob); return 5; }
        memcpy(out, &val, sizeof(val));
        out += sizeof(val);
      } else {
        const uint32_t val = static_cast<uint32_t>(face[j].value());
        if (out + sizeof(val) > out_end) { free(blob); return 5; }
        memcpy(out, &val, sizeof(val));
        out += sizeof(val);
      }
    }
  }

  for (const draco::PointAttribute *attr : sorted.attrs) {
    const int dim = attr->num_components();
    bool ok = false;
    switch (attr->data_type()) {
    case draco::DT_INT8:
      ok = write_attr<int8_t>(attr, dim, vertex_count, out, out_end);
      break;
    case draco::DT_UINT8:
      ok = write_attr<uint8_t>(attr, dim, vertex_count, out, out_end);
      break;
    case draco::DT_INT16:
      ok = write_attr<int16_t>(attr, dim, vertex_count, out, out_end);
      break;
    case draco::DT_UINT16:
      ok = write_attr<uint16_t>(attr, dim, vertex_count, out, out_end);
      break;
    case draco::DT_INT32:
      ok = write_attr<int32_t>(attr, dim, vertex_count, out, out_end);
      break;
    case draco::DT_UINT32:
      ok = write_attr<uint32_t>(attr, dim, vertex_count, out, out_end);
      break;
    case draco::DT_FLOAT32:
      ok = write_attr<float>(attr, dim, vertex_count, out, out_end);
      break;
    case draco::DT_FLOAT64:
      ok = write_attr<double>(attr, dim, vertex_count, out, out_end);
      break;
    default:
      ok = false;
      break;
    }
    if (!ok) {
      free(blob);
      return 5;
    }
  }

  *out_vertex_count = vertex_count;
  *out_index_count = index_count;
  *out_buffer_size = buffer_size;
  *out_attr_count = attr_count;
  *out_blob = blob;
  *out_written = static_cast<size_t>(out - blob);
  return 0;
}

extern "C" void draco_c_free(uint8_t *p) { free(p); }

extern "C" int32_t draco_c_position_quant_bounds(const uint8_t *data, size_t len,
                                      float *out_min3, float *out_max3) {
  if (!data || !out_min3 || !out_max3) {
    return 0;
  }
  draco::DecoderBuffer buffer;
  buffer.Init(reinterpret_cast<const char *>(data), len);
  draco::Decoder decoder;
  // Same as decoder_api.cc: keep the quantization transform on POSITION so
  // the authored bounds survive the decode.
  decoder.SetSkipAttributeTransform(draco::GeometryAttribute::POSITION);
  auto status_or_geometry = decoder.DecodeMeshFromBuffer(&buffer);
  if (!status_or_geometry.ok()) {
    return 0;
  }
  std::unique_ptr<draco::Mesh> mesh = std::move(status_or_geometry).value();
  if (!mesh) {
    return 0;
  }
  const draco::PointAttribute *pos_attr =
      mesh->GetNamedAttribute(draco::GeometryAttribute::POSITION);
  if (!pos_attr || pos_attr->num_components() != 3) {
    return 0;
  }
  const draco::AttributeTransformData *xform_data =
      pos_attr->GetAttributeTransformData();
  if (!xform_data || xform_data->transform_type() !=
                         draco::ATTRIBUTE_QUANTIZATION_TRANSFORM) {
    return 0;
  }
  draco::AttributeQuantizationTransform qtransform;
  if (!qtransform.InitFromAttribute(*pos_attr)) {
    return 0;
  }
  const float range = qtransform.range();
  for (int i = 0; i < 3; ++i) {
    const float mn = qtransform.min_value(i);
    out_min3[i] = mn;
    out_max3[i] = mn + range;
  }
  return 1;
}
