#include "decoder_api.h"
#include "draco_decoder/src/ffi.rs.h"

#include "draco/attributes/attribute_quantization_transform.h"
#include "draco/attributes/attribute_transform_type.h"
#include "draco/attributes/geometry_attribute.h"
#include "draco/attributes/point_attribute.h"
#include "draco/compression/decode.h"
#include "draco/compression/mesh/mesh_decoder.h"
#include "draco/compression/point_cloud/point_cloud_decoder.h"
#include "draco/core/decoder_buffer.h"
#include "draco/mesh/mesh.h"
#include "draco/point_cloud/point_cloud.h"
#include <memory>

// DracoMesh implementation
DracoMesh::DracoMesh(std::unique_ptr<draco::Mesh> m) : mesh(std::move(m)) {}
DracoMesh::~DracoMesh() = default;

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

rust::Vec<uint8_t> decode_point_cloud(rust::Slice<const uint8_t> data) {
  draco::DecoderBuffer buffer;
  buffer.Init(reinterpret_cast<const char *>(data.data()), data.size());

  draco::Decoder decoder;
  auto status_or_geometry = decoder.DecodePointCloudFromBuffer(&buffer);
  if (!status_or_geometry.ok()) {
    return {};
  }

  std::unique_ptr<draco::PointCloud> pc = std::move(status_or_geometry).value();

  const draco::PointAttribute *attr =
      pc->GetNamedAttribute(draco::GeometryAttribute::POSITION);
  if (!attr) {
    return {};
  }

  rust::Vec<uint8_t> out;
  for (draco::PointIndex i(0); i < pc->num_points(); ++i) {
    float point[3] = {0.0f};
    attr->GetValue(attr->mapped_index(i), &point[0]);
    uint8_t *ptr = reinterpret_cast<uint8_t *>(point);
    for (size_t j = 0; j < sizeof(point); ++j) {
      out.push_back(ptr[j]);
    }
  }
  return out;
}


std::unique_ptr<DracoMesh> create_mesh(rust::Slice<const uint8_t> data) {
  draco::DecoderBuffer buffer;
  buffer.Init(reinterpret_cast<const char *>(data.data()), data.size());

  draco::Decoder decoder;
  auto status_or_geometry = decoder.DecodeMeshFromBuffer(&buffer);
  if (!status_or_geometry.ok()) {
    return nullptr;
  }

  std::unique_ptr<draco::Mesh> mesh = std::move(status_or_geometry).value();
  return std::make_unique<DracoMesh>(std::move(mesh));
}

bool compute_mesh_config(const DracoMesh &draco_mesh, MeshConfig &config) {
  const draco::Mesh *mesh = draco_mesh.mesh.get();
  if (!mesh) {
    return false;
  }

  // Basic info
  config.vertex_count = mesh->num_points();
  config.index_count = mesh->num_faces() * 3;

  // Index length
  if (config.index_count <=
      static_cast<uint32_t>(std::numeric_limits<uint16_t>::max())) {
    config.index_length = config.index_count * sizeof(uint16_t);
  } else {
    config.index_length = config.index_count * sizeof(uint32_t);
  }

  // Collect and sort attributes by unique_id
  struct AttrEntry {
    const draco::PointAttribute *attr = nullptr;
    int unique_id = 0;
  };

  std::vector<AttrEntry> attrs;
  attrs.reserve(mesh->num_attributes());

  for (int i = 0; i < mesh->num_attributes(); ++i) {
    const draco::PointAttribute *attr = mesh->attribute(i);
    AttrEntry e;
    e.attr = attr;
    e.unique_id = attr->unique_id();
    attrs.push_back(e);
  }

  std::sort(attrs.begin(), attrs.end(),
            [](const AttrEntry &a, const AttrEntry &b) {
              return a.unique_id < b.unique_id;
            });

  // Calculate offsets and fill attribute info
  uint32_t current_offset = config.index_length;

  for (auto &entry : attrs) {
    const draco::PointAttribute *attr = entry.attr;
    MeshAttribute mesh_attr;

    mesh_attr.dim = attr->num_components();
    mesh_attr.unique_id = attr->unique_id();

    // Use Draco native DataType enum directly
    mesh_attr.data_type = static_cast<int>(attr->data_type());

    mesh_attr.offset = current_offset;
    mesh_attr.length = mesh_attr.dim * config.vertex_count *
                       sizeof_data_type(attr->data_type());

    config.attributes.push_back(mesh_attr);
    current_offset += mesh_attr.length;
  }

  config.buffer_size = current_offset;
  return true;
}

bool decode_position_quant_bounds(rust::Slice<const uint8_t> data,
                                  PositionQuantBounds &out) {
  out.valid = false;
  for (int i = 0; i < 3; ++i) {
    out.min[i] = 0.0f;
    out.max[i] = 0.0f;
  }

  draco::DecoderBuffer buffer;
  buffer.Init(reinterpret_cast<const char *>(data.data()), data.size());

  draco::Decoder decoder;
  // Preserve the AttributeQuantizationTransform parameters on the final
  // decoded POSITION attribute (otherwise the decoder destroys them after
  // dequantizing the int32 portable data to float32 values).
  decoder.SetSkipAttributeTransform(draco::GeometryAttribute::POSITION);

  auto status_or_geometry = decoder.DecodeMeshFromBuffer(&buffer);
  if (!status_or_geometry.ok()) {
    return false;
  }
  std::unique_ptr<draco::Mesh> mesh = std::move(status_or_geometry).value();
  if (!mesh) {
    return false;
  }

  const draco::PointAttribute *pos_attr =
      mesh->GetNamedAttribute(draco::GeometryAttribute::POSITION);
  if (!pos_attr) {
    return false;
  }
  if (pos_attr->num_components() != 3) {
    return false;
  }
  const draco::AttributeTransformData *xform_data =
      pos_attr->GetAttributeTransformData();
  if (!xform_data ||
      xform_data->transform_type() !=
          draco::ATTRIBUTE_QUANTIZATION_TRANSFORM) {
    // POSITION wasn't quantized — post-decode scan is already exact.
    return false;
  }

  draco::AttributeQuantizationTransform qtransform;
  if (!qtransform.InitFromAttribute(*pos_attr)) {
    return false;
  }

  const float range = qtransform.range();
  for (int i = 0; i < 3; ++i) {
    const float mn = qtransform.min_value(i);
    out.min[i] = mn;
    out.max[i] = mn + range;
  }
  out.valid = true;
  return true;
}

size_t decode_mesh_to_buffer(const DracoMesh &draco_mesh, uint8_t *out_ptr,
                             size_t out_len) {
  const draco::Mesh *mesh = draco_mesh.mesh.get();
  if (!mesh) {
    return 0;
  }

  uint8_t *out = out_ptr;
  const size_t out_end = reinterpret_cast<size_t>(out_ptr) + out_len;

  // Helper function to write scalar
  auto write_scalar = [&](const void *src, draco::DataType type) -> bool {
    size_t size = sizeof_data_type(type);
    if (reinterpret_cast<size_t>(out) + size > out_end)
      return false;
    memcpy(out, src, size);
    out += size;
    return true;
  };

  // Write indices
  const int num_faces = mesh->num_faces();
  bool use_u16 =
      (num_faces * 3 <= static_cast<int>(std::numeric_limits<uint16_t>::max()));

  if (use_u16) {
    for (draco::FaceIndex i(0); i < num_faces; ++i) {
      const auto &face = mesh->face(i);
      for (int j = 0; j < 3; ++j) {
        uint16_t val = static_cast<uint16_t>(face[j].value());
        if (reinterpret_cast<size_t>(out) + sizeof(uint16_t) > out_end)
          return 0;
        *reinterpret_cast<uint16_t *>(out) = val;
        out += sizeof(uint16_t);
      }
    }
  } else {
    for (draco::FaceIndex i(0); i < num_faces; ++i) {
      const auto &face = mesh->face(i);
      for (int j = 0; j < 3; ++j) {
        uint32_t val = static_cast<uint32_t>(face[j].value());
        if (reinterpret_cast<size_t>(out) + sizeof(uint32_t) > out_end)
          return 0;
        *reinterpret_cast<uint32_t *>(out) = val;
        out += sizeof(uint32_t);
      }
    }
  }

  // Sort and write attributes
  struct AttrEntry {
    const draco::PointAttribute *attr = nullptr;
    int unique_id = 0;
  };

  std::vector<AttrEntry> attrs;
  attrs.reserve(mesh->num_attributes());

  for (int i = 0; i < mesh->num_attributes(); ++i) {
    const draco::PointAttribute *attr = mesh->attribute(i);
    AttrEntry e;
    e.attr = attr;
    e.unique_id = attr->unique_id();
    attrs.push_back(e);
  }

  std::sort(attrs.begin(), attrs.end(),
            [](const AttrEntry &a, const AttrEntry &b) {
              return a.unique_id < b.unique_id;
            });

  int num_points = mesh->num_points();

  for (auto &entry : attrs) {
    const draco::PointAttribute *attr = entry.attr;
    int dim = attr->num_components();
    draco::DataType type = attr->data_type();

    for (draco::PointIndex j(0); j < num_points; ++j) {
      switch (type) {
      case draco::DT_INT8: {
        int8_t v[4] = {};
        attr->ConvertValue(attr->mapped_index(j), v);
        for (int k = 0; k < dim; ++k)
          if (!write_scalar(&v[k], type))
            return 0;
        break;
      }
      case draco::DT_UINT8: {
        uint8_t v[4] = {};
        attr->ConvertValue(attr->mapped_index(j), v);
        for (int k = 0; k < dim; ++k)
          if (!write_scalar(&v[k], type))
            return 0;
        break;
      }
      case draco::DT_INT16: {
        int16_t v[4] = {};
        attr->ConvertValue(attr->mapped_index(j), v);
        for (int k = 0; k < dim; ++k)
          if (!write_scalar(&v[k], type))
            return 0;
        break;
      }
      case draco::DT_UINT16: {
        uint16_t v[4] = {};
        attr->ConvertValue(attr->mapped_index(j), v);
        for (int k = 0; k < dim; ++k)
          if (!write_scalar(&v[k], type))
            return 0;
        break;
      }
      case draco::DT_INT32: {
        int32_t v[4] = {};
        attr->ConvertValue(attr->mapped_index(j), v);
        for (int k = 0; k < dim; ++k)
          if (!write_scalar(&v[k], type))
            return 0;
        break;
      }
      case draco::DT_UINT32: {
        uint32_t v[4] = {};
        attr->ConvertValue(attr->mapped_index(j), v);
        for (int k = 0; k < dim; ++k)
          if (!write_scalar(&v[k], type))
            return 0;
        break;
      }
      case draco::DT_FLOAT32: {
        float v[4] = {};
        attr->ConvertValue(attr->mapped_index(j), v);
        for (int k = 0; k < dim; ++k)
          if (!write_scalar(&v[k], type))
            return 0;
        break;
      }
      case draco::DT_FLOAT64: {
        double v[4] = {};
        attr->ConvertValue(attr->mapped_index(j), v);
        for (int k = 0; k < dim; ++k)
          if (!write_scalar(&v[k], type))
            return 0;
        break;
      }
      default:
        return 0;
      }
    }
  }

  return static_cast<size_t>(out - out_ptr);
}