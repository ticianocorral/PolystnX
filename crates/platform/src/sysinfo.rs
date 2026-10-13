//! System readouts for the dev-mode perf overlay: CPU temperature (thermal
//! zones of sysfs) and this process's resident RAM (/proc/self/status).
//! Linux-only sources; every read is best-effort `Option` — the overlay
//! just omits what the OS won't tell. Parsing is split into pure functions
//! so the tests can pin the formats without a live kernel.

/// CPU temperature in °C, from sysfs thermal zones. Prefers zones whose
/// `type` mentions cpu/soc (handhelds expose several zones — battery, GPU,
/// ACPI noise); falls back to the hottest zone of all. `None` outside Linux
/// or when no zone is readable.
pub fn cpu_temp_celsius() -> Option<f32> {
    #[cfg(target_os = "linux")]
    {
        let mut cpu_zone: Option<f32> = None;
        let mut any_zone: Option<f32> = None;
        let zones = std::fs::read_dir("/sys/class/thermal").ok()?;
        for zone in zones.flatten() {
            let dir = zone.path();
            let temp = parse_thermal_temp(&std::fs::read_to_string(
                dir.join("temp"),
            )
            .ok()?)?;
            if let Ok(kind) = std::fs::read_to_string(dir.join("type")) {
                let kind = kind.to_ascii_lowercase();
                if kind.contains("cpu") || kind.contains("soc") {
                    cpu_zone = Some(cpu_zone.map_or(temp, |c| c.max(temp)));
                    continue;
                }
            }
            any_zone = Some(any_zone.map_or(temp, |c| c.max(temp)));
        }
        cpu_zone.or(any_zone).map(|milli| milli / 1000.0)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Millidegrees string ("52000", trailing newline fine) → °C.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_thermal_temp(s: &str) -> Option<f32> {
    s.trim().parse::<f32>().ok().map(|milli| milli / 1000.0)
}

/// This process's resident RAM in MB, from /proc/self/status `VmRSS`.
/// `None` outside Linux or when the field is missing.
pub fn process_ram_mb() -> Option<f32> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        parse_vm_rss_kb(&status).map(|kb| kb / 1024.0)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// The `VmRSS:` line ("VmRSS:\t   84352 kB") → kB as f32.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_vm_rss_kb(status: &str) -> Option<f32> {
    let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
    line.split_whitespace().nth(1)?.parse::<f32>().ok()
}

#[cfg(test)]
mod tests {
    use super::{parse_thermal_temp, parse_vm_rss_kb};

    #[test]
    fn thermal_temp_parses_millidegrees_with_newline() {
        assert_eq!(parse_thermal_temp("52000\n"), Some(52.0));
        assert_eq!(parse_thermal_temp("47530"), Some(47.53));
        assert_eq!(parse_thermal_temp(""), None);
        assert_eq!(parse_thermal_temp("x"), None);
    }

    #[test]
    fn vm_rss_parses_the_kilobyte_field() {
        let status = "Name:\tpolystnx\nVmPeak:\t 999999 kB\nVmRSS:\t   84352 kB\nThreads:\t12\n";
        assert_eq!(parse_vm_rss_kb(status), Some(84352.0));
        assert_eq!(parse_vm_rss_kb("VmRSS: sem numero"), None);
        assert_eq!(parse_vm_rss_kb("Name:\tx"), None);
    }
}
