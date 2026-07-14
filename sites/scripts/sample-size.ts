export type SampleSizeInput = {
  baseline: number;
  mde: number;
  alpha?: number;
  power?: number;
};

export type SampleSizeResult = {
  perVariant: number;
  total: number;
  baseline: number;
  mde: number;
  alpha: number;
  power: number;
};

export function normInv(p: number): number {
  if (p <= 0 || p >= 1) {
    throw new RangeError(`normInv expects p in (0,1), got ${p}`);
  }
  const a = [
    -3.969683028665376e1, 2.209460984245205e2, -2.759285104469687e2,
    1.38357751867269e2, -3.066479806614716e1, 2.506628277459239,
  ];
  const b = [
    -5.447609879822406e1, 1.615858368580409e2, -1.556989798598866e2,
    6.680131188771972e1, -1.328068155288572e1,
  ];
  const c = [
    -7.784894002430293e-3, -3.223964580411365e-1, -2.400758277161838,
    -2.549732539343734, 4.374664141464968, 2.938163982698783,
  ];
  const d = [
    7.784695709041462e-3, 3.224671290700398e-1, 2.445134137142996,
    3.754408661907416,
  ];
  const pLow = 0.02425;
  const pHigh = 1 - pLow;

  let q: number;
  let r: number;
  if (p < pLow) {
    q = Math.sqrt(-2 * Math.log(p));
    return (
      (((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5]) /
      ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1)
    );
  }
  if (p <= pHigh) {
    q = p - 0.5;
    r = q * q;
    return (
      ((((((a[0] * r + a[1]) * r + a[2]) * r + a[3]) * r + a[4]) * r + a[5]) * q) /
      (((((b[0] * r + b[1]) * r + b[2]) * r + b[3]) * r + b[4]) * r + 1)
    );
  }
  q = Math.sqrt(-2 * Math.log(1 - p));
  return (
    -(((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5]) /
    ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1)
  );
}

export function sampleSize(input: SampleSizeInput): SampleSizeResult {
  const alpha = input.alpha ?? 0.05;
  const power = input.power ?? 0.8;
  const { baseline, mde } = input;

  if (!(baseline >= 0 && baseline < 1)) {
    throw new RangeError(`baseline must be in [0,1), got ${baseline}`);
  }
  if (!(mde > 0)) {
    throw new RangeError(`mde must be > 0, got ${mde}`);
  }
  const p1 = baseline;
  const p2 = baseline + mde;
  if (!(p2 > 0 && p2 < 1)) {
    throw new RangeError(`baseline + mde must be in (0,1), got ${p2}`);
  }
  if (!(alpha > 0 && alpha < 1)) {
    throw new RangeError(`alpha must be in (0,1), got ${alpha}`);
  }
  if (!(power > 0 && power < 1)) {
    throw new RangeError(`power must be in (0,1), got ${power}`);
  }

  const zAlpha = normInv(1 - alpha / 2);
  const zBeta = normInv(power);
  const pBar = (p1 + p2) / 2;

  const term1 = zAlpha * Math.sqrt(2 * pBar * (1 - pBar));
  const term2 = zBeta * Math.sqrt(p1 * (1 - p1) + p2 * (1 - p2));
  const numerator = (term1 + term2) ** 2;
  const denominator = (p2 - p1) ** 2;

  const perVariant = Math.ceil(numerator / denominator);
  return {
    perVariant,
    total: perVariant * 2,
    baseline,
    mde,
    alpha,
    power,
  };
}

function parseArgs(argv: string[]): SampleSizeInput {
  const flags: Record<string, string> = {};
  const positional: string[] = [];
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a.startsWith("--")) {
      const key = a.slice(2);
      const next = argv[i + 1];
      if (next !== undefined && !next.startsWith("--")) {
        flags[key] = next;
        i++;
      } else {
        flags[key] = "true";
      }
    } else {
      positional.push(a);
    }
  }
  const num = (s: string | undefined): number | undefined =>
    s === undefined ? undefined : Number(s);

  const baseline = num(flags.baseline) ?? num(positional[0]);
  const mde = num(flags.mde) ?? num(positional[1]);
  if (baseline === undefined || Number.isNaN(baseline)) {
    throw new Error("missing or invalid --baseline");
  }
  if (mde === undefined || Number.isNaN(mde)) {
    throw new Error("missing or invalid --mde");
  }
  return {
    baseline,
    mde,
    alpha: num(flags.alpha),
    power: num(flags.power),
  };
}

function isMain(): boolean {
  try {
    const entry = process.argv[1] ?? "";
    return import.meta.url === `file://${entry}` || entry.endsWith("sample-size.ts");
  } catch {
    return false;
  }
}

if (isMain()) {
  try {
    const input = parseArgs(process.argv.slice(2));
    const result = sampleSize(input);
    process.stderr.write(
      `two-proportion test  \u{3B1}=${result.alpha}  power=${result.power}\n` +
        `baseline=${result.baseline}  mde=${result.mde}  ` +
        `(target=${result.baseline + result.mde})\n` +
        `min sample per variant: ${result.perVariant}\n` +
        `total (2 arms):         ${result.total}\n`,
    );
    process.stdout.write(JSON.stringify(result) + "\n");
  } catch (err) {
    process.stderr.write(
      `sample-size error: ${err instanceof Error ? err.message : String(err)}\n` +
        `usage: tsx scripts/sample-size.ts --baseline <0..1> --mde <abs diff> ` +
        `[--alpha 0.05] [--power 0.8]\n`,
    );
    process.exit(1);
  }
}
