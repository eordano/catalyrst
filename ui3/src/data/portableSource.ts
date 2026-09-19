export function portableSource(input: string): string {
  const value = input.trim();
  if (/^[a-z0-9][a-z0-9-]*\.dcl\.eth$/i.test(value)) return value.toLowerCase();
  if (!/[\s"'\\]/.test(value)) {
    try {
      const url = new URL(value);
      if (["https:", "http:"].includes(url.protocol) && !url.username && !url.password)
        return url.href;
    } catch {}
  }
  throw new Error("Enter a .dcl.eth world name or an HTTP(S) realm URL.");
}
