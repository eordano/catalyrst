/* jpeg9c_wrapper.c — thin C wrapper around IJG libjpeg 9c's public API.
 *
 * Exposes a single decode entry point that mirrors how FreeImage 3.18.0's
 * bundled libjpeg (version 9c, 14-Jan-2018) decodes a JPEG for Unity's editor
 * AssetImporter:
 *   - islow IDCT (JDCT_ISLOW, the libjpeg default)
 *   - box (non-fancy) chroma upsampling
 *   - JFIF / BT.601 full-range YCbCr->RGB
 * Output is RGBA8 (alpha forced to 255).
 *
 * This wrapper is original clean-room code; it only CALLS the permissively
 * licensed IJG libjpeg public API. The IJG license permits use, modification
 * and redistribution.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef __wasm__
/* wasm32 has no native setjmp/longjmp. A libjpeg fatal error is instead
 * routed into a noreturn Rust hook (panic -> host trap); the whole
 * conversion aborts rather than recovering per-image. */
typedef int jmp_buf[1];
extern void jpeg9c_wasm_fatal(void) __attribute__((noreturn));
#define setjmp(env) 0
#define longjmp(env, v) jpeg9c_wasm_fatal()
#else
#include <setjmp.h>
#endif
#include "jpeglib.h"

struct jpeg9c_error_mgr {
    struct jpeg_error_mgr pub;
    jmp_buf setjmp_buffer;
};

static void jpeg9c_error_exit(j_common_ptr cinfo) {
    struct jpeg9c_error_mgr *err = (struct jpeg9c_error_mgr *)cinfo->err;
    longjmp(err->setjmp_buffer, 1);
}

static void jpeg9c_emit_message(j_common_ptr cinfo, int msg_level) {
    (void)cinfo; (void)msg_level;
}

/* Decode a JPEG buffer to a freshly malloc'd RGBA8 buffer.
 * On success returns the buffer (caller frees with jpeg9c_free), sets *w/*h.
 * On failure returns NULL.
 * fancy_upsampling: 0 = box (standalone/editor path), 1 = fancy (glTFast path). */
unsigned char *jpeg9c_decode_rgba(const unsigned char *data, unsigned long len,
                                  int *w, int *h, int fancy_upsampling) {
    struct jpeg_decompress_struct cinfo;
    struct jpeg9c_error_mgr jerr;
    unsigned char *rgba = NULL;
    unsigned char *rowbuf = NULL;

    cinfo.err = jpeg_std_error(&jerr.pub);
    jerr.pub.error_exit = jpeg9c_error_exit;
    jerr.pub.emit_message = jpeg9c_emit_message;
    if (setjmp(jerr.setjmp_buffer)) {
        if (rgba) { free(rgba); rgba = NULL; }
        if (rowbuf) free(rowbuf);
        jpeg_destroy_decompress(&cinfo);
        return NULL;
    }

    jpeg_create_decompress(&cinfo);
    jpeg_mem_src(&cinfo, (unsigned char *)data, len);
    jpeg_read_header(&cinfo, TRUE);

    cinfo.dct_method = JDCT_ISLOW;
    cinfo.do_fancy_upsampling = fancy_upsampling ? TRUE : FALSE;
    cinfo.out_color_space = JCS_RGB;

    jpeg_start_decompress(&cinfo);
    int width = cinfo.output_width;
    int height = cinfo.output_height;
    int comps = cinfo.output_components;

    rgba = (unsigned char *)malloc((size_t)width * height * 4);
    if (!rgba) { longjmp(jerr.setjmp_buffer, 1); }
    rowbuf = (unsigned char *)malloc((size_t)width * comps);
    if (!rowbuf) { longjmp(jerr.setjmp_buffer, 1); }

    while (cinfo.output_scanline < cinfo.output_height) {
        int y = cinfo.output_scanline;
        unsigned char *rp = rowbuf;
        jpeg_read_scanlines(&cinfo, &rp, 1);
        unsigned char *dst = rgba + (size_t)y * width * 4;
        for (int x = 0; x < width; x++) {
            if (comps >= 3) {
                dst[x*4+0] = rowbuf[x*comps+0];
                dst[x*4+1] = rowbuf[x*comps+1];
                dst[x*4+2] = rowbuf[x*comps+2];
            } else {
                unsigned char g = rowbuf[x*comps+0];
                dst[x*4+0] = g; dst[x*4+1] = g; dst[x*4+2] = g;
            }
            dst[x*4+3] = 255;
        }
    }

    jpeg_finish_decompress(&cinfo);
    jpeg_destroy_decompress(&cinfo);
    free(rowbuf);
    *w = width;
    *h = height;
    return rgba;
}

void jpeg9c_free(unsigned char *p) { free(p); }
