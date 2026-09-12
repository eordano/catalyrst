export function suffixLabel(base: string, suffix?: string): string {
  return suffix ? `${base} ${suffix}` : base;
}

export type LabelSuffixProps = {
  labelSuffix?: string;
};
