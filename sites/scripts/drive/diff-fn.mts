// The pixel diff both visual-regression tools run, as source text evaluated in a
// headless page: shot.mts (route screenshots) and tools/story-shots/diff.mts
// (per-story). In-browser OffscreenCanvas; a channel delta > 12 counts as a
// diff pixel; returns the red-overlay diff image. Its own module because
// shot.mts runs main() at import.
export const DIFF_FN = `async (a64, b64) => {
  const load = (b64) => new Promise((res, rej) => {
    const img = new Image();
    img.onload = () => res(img);
    img.onerror = rej;
    img.src = 'data:image/png;base64,' + b64;
  });
  const [ia, ib] = await Promise.all([load(a64), load(b64)]);
  const w = Math.max(ia.width, ib.width), h = Math.max(ia.height, ib.height);
  const cv = (img) => {
    const c = new OffscreenCanvas(w, h);
    const ctx = c.getContext('2d');
    ctx.drawImage(img, 0, 0);
    return ctx.getImageData(0, 0, w, h).data;
  };
  const da = cv(ia), db = cv(ib);
  const out = new OffscreenCanvas(w, h);
  const octx = out.getContext('2d');
  const od = octx.createImageData(w, h);
  let diff = 0;
  for (let i = 0; i < da.length; i += 4) {
    const delta = Math.max(
      Math.abs(da[i] - db[i]), Math.abs(da[i+1] - db[i+1]),
      Math.abs(da[i+2] - db[i+2]));
    if (delta > 12) {
      diff++;
      od.data[i] = 255; od.data[i+3] = 255;
    } else {
      od.data[i] = da[i]; od.data[i+1] = da[i+1];
      od.data[i+2] = da[i+2]; od.data[i+3] = 60;
    }
  }
  octx.putImageData(od, 0, 0);
  const blob = await out.convertToBlob({ type: 'image/png' });
  const buf = await blob.arrayBuffer();
  let bin = '';
  const bytes = new Uint8Array(buf);
  for (let i = 0; i < bytes.length; i += 0x8000) {
    bin += String.fromCharCode.apply(null, bytes.subarray(i, i + 0x8000));
  }
  return { pct: (100 * diff) / (w * h), diffB64: btoa(bin) };
}`;
