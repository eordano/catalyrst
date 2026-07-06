#![no_std]
#![feature(abi_ptx, stdarch_nvptx)]
#[path = "core/mod.rs"]
mod abgen_gpu_core;

use crate::abgen_gpu_core::bc7::{encode_group, group_signature, OptTables, Params, GROUP_WIDTH};
use crate::abgen_gpu_core::mips::{
    box_halve_cell, box_halve_dims, level_block_dims, linearize_pixel, quantize_pack_block,
    HalveItem, LinItem, PackItem,
};
use core::arch::nvptx::{_block_dim_x, _block_idx_x, _thread_idx_x};

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::arch::nvptx::trap() }
}

unsafe fn global_id() -> usize {
    (_block_idx_x() as usize) * (_block_dim_x() as usize) + (_thread_idx_x() as usize)
}

unsafe fn find_item(prefix: *const u64, n_items: usize, gid: u64) -> usize {
    let mut lo = 0usize;
    let mut hi = n_items;
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if *prefix.add(mid) <= gid {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

#[no_mangle]
pub unsafe extern "ptx-kernel" fn bc7_encode_groups(
    blocks: *const u8,
    num_blocks: usize,
    params: *const Params,
    tables: *const OptTables,
    out: *mut u8,
) {
    let gidx =
        (_block_idx_x() as usize) * (_block_dim_x() as usize) + (_thread_idx_x() as usize);
    let num_groups = num_blocks.div_ceil(GROUP_WIDTH);
    if gidx >= num_groups {
        return;
    }
    let start = gidx * GROUP_WIDTH;
    let n = if num_blocks - start < GROUP_WIDTH {
        num_blocks - start
    } else {
        GROUP_WIDTH
    };
    let src = core::slice::from_raw_parts(blocks.add(start * 64), n * 64);
    let mut enc = [[0u8; 16]; GROUP_WIDTH];
    encode_group(src, n, &*params, &*tables, &mut enc);
    let dst = core::slice::from_raw_parts_mut(out.add(start * 16), n * 16);
    for k in 0..n {
        dst[k * 16..(k + 1) * 16].copy_from_slice(&enc[k]);
    }
}

#[no_mangle]
pub unsafe extern "ptx-kernel" fn bc7_group_sigs(
    blocks: *const u8,
    num_blocks: usize,
    sigs: *mut u8,
) {
    let gidx = global_id();
    let num_groups = num_blocks.div_ceil(GROUP_WIDTH);
    if gidx >= num_groups {
        return;
    }
    let start = gidx * GROUP_WIDTH;
    let n = if num_blocks - start < GROUP_WIDTH {
        num_blocks - start
    } else {
        GROUP_WIDTH
    };
    let src = core::slice::from_raw_parts(blocks.add(start * 64), n * 64);
    *sigs.add(gidx) = group_signature(src, n);
}

#[no_mangle]
pub unsafe extern "ptx-kernel" fn bc7_encode_groups_perm(
    blocks: *const u8,
    num_blocks: usize,
    perm: *const u32,
    params: *const Params,
    tables: *const OptTables,
    out: *mut u8,
) {
    let i = global_id();
    let num_groups = num_blocks.div_ceil(GROUP_WIDTH);
    if i >= num_groups {
        return;
    }
    let gidx = *perm.add(i) as usize;
    let start = gidx * GROUP_WIDTH;
    let n = if num_blocks - start < GROUP_WIDTH {
        num_blocks - start
    } else {
        GROUP_WIDTH
    };
    let src = core::slice::from_raw_parts(blocks.add(start * 64), n * 64);
    let mut enc = [[0u8; 16]; GROUP_WIDTH];
    encode_group(src, n, &*params, &*tables, &mut enc);
    let dst = core::slice::from_raw_parts_mut(out.add(start * 16), n * 16);
    for k in 0..n {
        dst[k * 16..(k + 1) * 16].copy_from_slice(&enc[k]);
    }
}

#[no_mangle]
pub unsafe extern "ptx-kernel" fn bc7_group_sigs_desc(
    blocks: *const u8,
    descs: *const u64,
    num_groups: usize,
    sigs: *mut u8,
) {
    let gidx = global_id();
    if gidx >= num_groups {
        return;
    }
    let d = *descs.add(gidx);
    let n = (d & 0xf) as usize;
    let start = (d >> 8) as usize;
    let src = core::slice::from_raw_parts(blocks.add(start * 64), n * 64);
    *sigs.add(gidx) = group_signature(src, n);
}

#[no_mangle]
pub unsafe extern "ptx-kernel" fn bc7_encode_groups_desc(
    blocks: *const u8,
    descs: *const u64,
    num_groups: usize,
    params4: *const Params,
    tables: *const OptTables,
    out: *mut u8,
) {
    let gidx = global_id();
    if gidx >= num_groups {
        return;
    }
    let d = *descs.add(gidx);
    let n = (d & 0xf) as usize;
    let bucket = ((d >> 4) & 0xf) as usize;
    let start = (d >> 8) as usize;
    let src = core::slice::from_raw_parts(blocks.add(start * 64), n * 64);
    let mut enc = [[0u8; 16]; GROUP_WIDTH];
    encode_group(src, n, &*params4.add(bucket), &*tables, &mut enc);
    let dst = core::slice::from_raw_parts_mut(out.add(start * 16), n * 16);
    for k in 0..n {
        dst[k * 16..(k + 1) * 16].copy_from_slice(&enc[k]);
    }
}

#[no_mangle]
pub unsafe extern "ptx-kernel" fn blockify_linearize(
    items: *const LinItem,
    prefix: *const u64,
    n_items: usize,
    total: usize,
    base: *const u8,
    pyr: *mut f32,
) {
    let gid = global_id();
    if gid >= total {
        return;
    }
    let idx = find_item(prefix, n_items, gid as u64);
    let it = &*items.add(idx);
    let p = gid as u64 - *prefix.add(idx);
    let src = core::slice::from_raw_parts(base.add(((it.base_px + p) * 4) as usize), 4);
    let dst = core::slice::from_raw_parts_mut(pyr.add(((it.pyr_px + p) * 4) as usize), 4);
    linearize_pixel(src, it.srgb != 0, dst);
}

#[no_mangle]
pub unsafe extern "ptx-kernel" fn blockify_quantize_pack(
    items: *const PackItem,
    prefix: *const u64,
    n_items: usize,
    total: usize,
    pyr: *const f32,
    blocks: *mut u8,
) {
    let gid = global_id();
    if gid >= total {
        return;
    }
    let idx = find_item(prefix, n_items, gid as u64);
    let it = &*items.add(idx);
    let lb = gid as u64 - *prefix.add(idx);
    let w = it.w as usize;
    let h = it.h as usize;
    let (bw, _) = level_block_dims(w, h);
    let bx = (lb as usize) % bw;
    let by = (lb as usize) / bw;
    let level = core::slice::from_raw_parts(pyr.add((it.lvl_px * 4) as usize), w * h * 4);
    let out = core::slice::from_raw_parts_mut(blocks.add(((it.blk_off + lb) * 64) as usize), 64);
    quantize_pack_block(level, w, h, it.srgb != 0, bx, by, out);
}

#[no_mangle]
pub unsafe extern "ptx-kernel" fn blockify_halve(
    items: *const HalveItem,
    prefix: *const u64,
    n_items: usize,
    total: usize,
    pyr: *mut f32,
) {
    let gid = global_id();
    if gid >= total {
        return;
    }
    let idx = find_item(prefix, n_items, gid as u64);
    let it = &*items.add(idx);
    let np = gid as u64 - *prefix.add(idx);
    let w = it.w as usize;
    let h = it.h as usize;
    let (nw, _) = box_halve_dims(w, h);
    let nx = (np as usize) % nw;
    let ny = (np as usize) / nw;
    let src = core::slice::from_raw_parts(pyr.add((it.src_px * 4) as usize) as *const f32, w * h * 4);
    let mut cell = [0f32; 4];
    box_halve_cell(src, w, h, nx, ny, &mut cell);
    let dst = core::slice::from_raw_parts_mut(pyr.add(((it.dst_px + np) * 4) as usize), 4);
    dst.copy_from_slice(&cell);
}
