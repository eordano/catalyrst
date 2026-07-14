type SectionIconProps = {
  n?: number | string;
  number?: number | string;
  validated?: boolean;
  size?: number;
  className?: string;
};

export default function SectionIcon({ n, number, validated = false, size = 34, className }: SectionIconProps) {
  const value = n != null ? n : number;
  const height = (size / 34) * 42;
  return (
    <svg
      viewBox="0 0 34 42"
      width={size}
      height={height}
      fill="none"
      aria-hidden="true"
      className={className}
    >
      <circle cx="16" cy="21" r="15.5" stroke="#736e7d" strokeOpacity="0.32" />
      <text
        x="16"
        y="21"
        textAnchor="middle"
        dominantBaseline="central"
        fontSize="15"
        fontWeight="600"
        fill="currentColor"
      >
        {value}
      </text>
      {validated && (
        <>
          <circle cx="27" cy="32" r="6" fill="var(--success)" stroke="#fff" strokeWidth="2" />
          <path d="M25 32C26 33 26.5 33.5 26.5 33.5L29.5 30.5" stroke="#fff" />
        </>
      )}
    </svg>
  );
}
