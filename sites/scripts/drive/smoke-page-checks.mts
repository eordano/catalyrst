export const PAGE_CHECKS = `(() => {
  const broken = [...document.images]
    .filter((i) => {
      if (!i.src || (i.complete && i.naturalWidth > 0)) return false;
      const r = i.getBoundingClientRect();
      const visible = r.bottom > 0 && r.top < innerHeight && r.right > 0 && r.left < innerWidth;
      return i.complete || i.loading !== 'lazy' || visible;
    })
    .map((i) => i.currentSrc || i.src);
  let fontOk = true;
  try { fontOk = document.fonts.check('16px Inter'); } catch {}
  return JSON.stringify({
    broken: broken.slice(0, 100),
    brokenTotal: broken.length,
    fontOk,
    title: document.title,
    bodyChars: (document.body?.innerText ?? '').length,
  });
})()`;
