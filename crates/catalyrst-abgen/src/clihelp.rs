pub fn version_line(bin: &str) -> String {
    format!(
        "{bin} {} ({})",
        env!("CARGO_PKG_VERSION"),
        option_env!("ABGEN_GIT_COMMIT").unwrap_or("unknown")
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub fn print_version(bin: &str) -> ! {
    println!("{}", version_line(bin));
    std::process::exit(0);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn print_help(usage: &str) -> ! {
    println!("{}", usage.trim_end());
    std::process::exit(0);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn usage_error(usage: &str) -> ! {
    eprintln!("{}", usage.trim_end());
    std::process::exit(2);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn bad_flag(flag: &str, usage: &str) -> ! {
    eprintln!("unknown option: {flag}");
    usage_error(usage);
}

pub fn bool_token(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

pub fn env_bool(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Err(_) => default,
        Ok(v) => {
            let t = v.trim();
            if t.is_empty() {
                default
            } else {
                bool_token(t).unwrap_or_else(|| {
                    eprintln!(
                        "warning: {name}={t}: unrecognized boolean (use 1/true/yes/on or 0/false/no/off); keeping default {default}"
                    );
                    default
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_line_shape() {
        let v = version_line("abgen");
        assert!(v.starts_with("abgen "));
        assert!(v.contains(env!("CARGO_PKG_VERSION")));
        assert!(v.ends_with(')'));
    }

    #[test]
    fn bool_tokens() {
        for t in ["1", "true", "YES", "On"] {
            assert_eq!(bool_token(t), Some(true));
        }
        for t in ["0", "false", "NO", "Off"] {
            assert_eq!(bool_token(t), Some(false));
        }
        assert_eq!(bool_token("maybe"), None);
        assert_eq!(bool_token(""), None);
    }

    #[test]
    fn env_bool_grammar() {
        let k = "ABGEN_TEST_ENV_BOOL_GRAMMAR";
        std::env::remove_var(k);
        assert!(!env_bool(k, false));
        assert!(env_bool(k, true));
        std::env::set_var(k, "1");
        assert!(env_bool(k, false));
        std::env::set_var(k, "off");
        assert!(!env_bool(k, true));
        std::env::set_var(k, "some-other-value");
        assert!(!env_bool(k, false));
        assert!(env_bool(k, true));
        std::env::set_var(k, "  ");
        assert!(!env_bool(k, false));
        assert!(env_bool(k, true));
        std::env::remove_var(k);
    }
}
