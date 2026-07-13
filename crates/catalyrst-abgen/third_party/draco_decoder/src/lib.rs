#[cfg(not(target_arch = "wasm32"))]
mod ffi;
#[cfg(target_arch = "wasm32")]
mod ffi_c;
pub mod utils;

pub use utils::{
    AttributeDataType, AttributeValues, DracoDecodeConfig, MeshAttribute, MeshDecodeResult,
};

#[cfg(not(target_arch = "wasm32"))]
pub async fn decode_mesh_with_config(data: &[u8]) -> Option<MeshDecodeResult> {
    ffi::decode_mesh_with_config(data)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn decode_mesh_with_config_sync(data: &[u8]) -> Option<MeshDecodeResult> {
    ffi::decode_mesh_with_config(data)
}

#[cfg(target_arch = "wasm32")]
pub async fn decode_mesh_with_config(data: &[u8]) -> Option<MeshDecodeResult> {
    ffi_c::decode_mesh_with_config(data)
}

#[cfg(target_arch = "wasm32")]
pub fn decode_mesh_with_config_sync(data: &[u8]) -> Option<MeshDecodeResult> {
    ffi_c::decode_mesh_with_config(data)
}

#[cfg(test)]
mod tests {

    #[cfg(not(target_arch = "wasm32"))]
    use super::ffi::decode_point_cloud_native;
    use std::collections::HashSet;
    use std::fs::{self};

    fn quantize(v: &[f32]) -> [i32; 3] {
        [
            (v[0] * 1000.0).round() as i32,
            (v[1] * 1000.0).round() as i32,
            (v[2] * 1000.0).round() as i32,
        ]
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore = "needs the un-vendored assets/pointcloud.drc fixture; run with --ignored once present"]
    fn test_decode_point_cloud() {
        let input = fs::read("assets/pointcloud.drc").expect("Failed to read pointcloud.drc");
        let output = decode_point_cloud_native(&input);

        assert!(
            output.len().is_multiple_of(12),
            "Expected output to be a multiple of 12 bytes (3 floats per point)"
        );

        let floats: Vec<f32> = output
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();

        let actual: HashSet<[i32; 3]> = floats.chunks_exact(3).map(quantize).collect();

        let expected: HashSet<[i32; 3]> = [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0], [2.0, 2.0, 2.0]]
            .iter()
            .map(|v| quantize(v))
            .collect();

        assert_eq!(
            actual, expected,
            "Decoded point cloud points do not match expected"
        );
    }

    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::*;

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    #[ignore = "needs the un-vendored assets/20/20_data.bin fixture; run with --ignored once present"]
    async fn test_decode_mesh_with_config() {
        use crate::{MeshDecodeResult, decode_mesh_with_config};

        let input = fs::read("assets/20/20_data.bin").expect("Failed to read model file");

        let decode_result = decode_mesh_with_config(&input).await;

        if let Some(MeshDecodeResult { data, config }) = decode_result {
            assert_eq!(config.vertex_count(), 3254);
            assert_eq!(config.index_count(), 4368);
            assert_eq!(config.attributes().len(), 3);

            assert_eq!(
                config.buffer_size(),
                config.index_length() as usize
                    + config
                        .attributes()
                        .iter()
                        .map(|a| a.lenght() as usize)
                        .sum::<usize>()
            );

            fs::create_dir_all("assets/20_decode").ok();
            let path = "assets/20_decode/20_data.bin";
            fs::write(path, &data).expect("Failed to write decoded mesh binary");
            println!("Wrote decoded mesh to {path}");
        }
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test]
    async fn test_decode_mesh_with_config_wasm() {
        use crate::decode_mesh_with_config;
        use wasm_bindgen::JsCast;
        use wasm_bindgen_futures::JsFuture;
        use web_sys::{Request, RequestInit, RequestMode, Response};

        let mut opts = RequestInit::new();
        opts.set_method("GET");
        opts.set_mode(RequestMode::Cors);

        let request = Request::new_with_str_and_init("assets/20/20_data.bin", &opts).unwrap();
        let resp_value = JsFuture::from(web_sys::window().unwrap().fetch_with_request(&request))
            .await
            .unwrap();
        let resp: Response = resp_value.dyn_into().unwrap();
        let buf = JsFuture::from(resp.array_buffer().unwrap()).await.unwrap();
        let u8_array = js_sys::Uint8Array::new(&buf);
        let mut input = vec![0; u8_array.length() as usize];
        u8_array.copy_to(&mut input[..]);

        let result = decode_mesh_with_config(&input).await;

        assert!(result.is_some(), "Decoding should succeed");
        let mesh = result.unwrap();

        web_sys::console::log_1(&format!("vertex_count: {}", mesh.config.vertex_count()).into());
        web_sys::console::log_1(&format!("index_count: {}", mesh.config.index_count()).into());
        web_sys::console::log_1(&format!("buffer_size: {}", mesh.config.buffer_size()).into());
        web_sys::console::log_1(&format!("attributes: {}", mesh.config.attributes().len()).into());

        assert_eq!(mesh.config.vertex_count(), 3254);
        assert_eq!(mesh.config.index_count(), 4368);
        assert_eq!(mesh.config.attributes().len(), 3);
        assert_eq!(mesh.data.len(), mesh.config.buffer_size());
    }
}
