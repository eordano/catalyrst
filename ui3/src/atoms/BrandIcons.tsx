type IconProps = { size?: number; className?: string };

export function GoogleIcon({ size = 20, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" className={className} aria-hidden="true">
      <path fill="#4285F4" d="M23.5 12.3c0-.8-.1-1.6-.2-2.3H12v4.5h6.5a5.6 5.6 0 0 1-2.4 3.6v3h3.9c2.3-2.1 3.5-5.2 3.5-8.8z" />
      <path fill="#34A853" d="M12 24c3.2 0 6-1.1 8-3l-3.9-3c-1.1.7-2.5 1.2-4.1 1.2-3.1 0-5.8-2.1-6.7-5H1.3v3.1A12 12 0 0 0 12 24z" />
      <path fill="#FBBC05" d="M5.3 14.3a7.2 7.2 0 0 1 0-4.6V6.6H1.3a12 12 0 0 0 0 10.8l4-3.1z" />
      <path fill="#EA4335" d="M12 4.8c1.8 0 3.3.6 4.6 1.8l3.4-3.4A12 12 0 0 0 1.3 6.6l4 3.1C6.2 6.9 8.9 4.8 12 4.8z" />
    </svg>
  );
}

export function AppleIcon({ size = 20, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" className={className} aria-hidden="true">
      <path d="M17.05 12.54c-.02-2.3 1.88-3.4 1.96-3.46-1.07-1.56-2.73-1.78-3.32-1.8-1.41-.14-2.76.83-3.48.83-.72 0-1.83-.81-3-.79-1.55.02-2.97.9-3.77 2.28-1.61 2.8-.41 6.94 1.15 9.21.76 1.11 1.67 2.36 2.86 2.31 1.15-.05 1.58-.74 2.97-.74 1.38 0 1.77.74 2.98.72 1.23-.02 2.01-1.13 2.76-2.25.87-1.29 1.23-2.54 1.25-2.6-.03-.01-2.4-.92-2.42-3.65zM14.77 5.6c.64-.77 1.07-1.85.95-2.92-.92.04-2.03.61-2.69 1.38-.59.68-1.11 1.78-.97 2.83 1.03.08 2.07-.52 2.71-1.29z" />
    </svg>
  );
}

export function DiscordIcon({ size = 20, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="#5865F2" className={className} aria-hidden="true">
      <path d="M19.6 4.6A18 18 0 0 0 15.1 3.2l-.2.4a16.7 16.7 0 0 1 4 1.3 15.1 15.1 0 0 0-12 0 16.7 16.7 0 0 1 4-1.3l-.2-.4A18 18 0 0 0 4.4 4.6 18.9 18.9 0 0 0 1.2 17.2 18.1 18.1 0 0 0 6.7 20l.4-.6a11.9 11.9 0 0 1-1.9-.9l.5-.4a12.9 12.9 0 0 0 10.6 0l.5.4a11.9 11.9 0 0 1-1.9.9l.4.6a18 18 0 0 0 5.5-2.8 18.9 18.9 0 0 0-3.2-12.6ZM8.4 14.6c-.9 0-1.6-.8-1.6-1.8s.7-1.8 1.6-1.8 1.6.8 1.6 1.8-.7 1.8-1.6 1.8Zm7.2 0c-.9 0-1.6-.8-1.6-1.8s.7-1.8 1.6-1.8 1.6.8 1.6 1.8-.7 1.8-1.6 1.8Z" />
    </svg>
  );
}

export function EmailIcon({ size = 20, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" className={className} aria-hidden="true">
      <rect x="2.5" y="4.5" width="19" height="15" rx="2.5" />
      <path d="M3 6.5l9 6 9-6" />
    </svg>
  );
}

export function WalletIcon({ size = 20, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" className={className} aria-hidden="true">
      <path d="M3 7.5A2.5 2.5 0 0 1 5.5 5h12A1.5 1.5 0 0 1 19 6.5V8" />
      <rect x="3" y="7.5" width="18" height="12" rx="2.5" />
      <path d="M16 13.5h3" />
      <circle cx="16.5" cy="13.5" r="1.4" fill="currentColor" stroke="none" />
    </svg>
  );
}
