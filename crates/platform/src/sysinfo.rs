//! System readouts for the dev-mode perf overlay: CPU temperature and this
//! process's resident RAM. Every source is best-effort `Option` — the
//! overlay just omits what the OS won't tell. Parsing is split into pure
//! functions so the tests can pin the formats without a live kernel.
//!
//! - Linux: thermal zones of sysfs + `VmRSS` de /proc/self/status.
//! - macOS: SMC user client (IOKit, chaves de temperatura — a lista cobre
//!   Intel e Apple Silicon; chave inexistente no modelo só falha e passa) +
//!   `ps -o rss=` do próprio processo.

/// CPU temperature in °C. Linux prefers zones whose `type` mentions
/// cpu/soc (handhelds expose several — battery, GPU, ACPI noise), falling
/// back to the hottest zone; macOS probes the usual SMC keys.
pub fn cpu_temp_celsius() -> Option<f32> {
    #[cfg(target_os = "linux")]
    {
        // Duas fontes: thermal_zone (ACPI/soc) e hwmon (k10temp, coretemp,
        // zenpower — em vários handhelds SÓ o hwmon tem a temperatura da
        // CPU). hwmon vence (é a leitura da CPU de verdade), zone reserva.
        let mut cpu_hwmon: Option<f32> = None;
        if let Ok(chips) = std::fs::read_dir("/sys/class/hwmon") {
            for chip in chips.flatten() {
                let dir = chip.path();
                let Ok(name) = std::fs::read_to_string(dir.join("name")) else {
                    continue;
                };
                let name = name.to_ascii_lowercase();
                let is_cpu = ["k10temp", "coretemp", "zenpower", "cpu_thermal", "x86_pkg_temp"]
                    .iter()
                    .any(|k| name.contains(k));
                if !is_cpu {
                    continue;
                }
                if let Ok(inputs) = std::fs::read_dir(&dir) {
                    for input in inputs.flatten() {
                        let fname = input.file_name().to_string_lossy().to_string();
                        if fname.starts_with("temp") && fname.ends_with("_input") {
                            if let Some(temp) = parse_thermal_temp(
                                &std::fs::read_to_string(input.path()).ok().unwrap_or_default(),
                            ) {
                                cpu_hwmon = Some(cpu_hwmon.map_or(temp, |c| c.max(temp)));
                            }
                        }
                    }
                }
            }
        }
        if cpu_hwmon.is_some() {
            return cpu_hwmon;
        }
        let mut cpu_zone: Option<f32> = None;
        let mut any_zone: Option<f32> = None;
        if let Ok(zones) = std::fs::read_dir("/sys/class/thermal") {
            for zone in zones.flatten() {
                let dir = zone.path();
                let Some(temp) =
                    parse_thermal_temp(&std::fs::read_to_string(dir.join("temp")).ok()?)
                else {
                    continue;
                };
                if let Ok(kind) = std::fs::read_to_string(dir.join("type")) {
                    let kind = kind.to_ascii_lowercase();
                    if kind.contains("cpu") || kind.contains("soc") {
                        cpu_zone = Some(cpu_zone.map_or(temp, |c| c.max(temp)));
                        continue;
                    }
                }
                any_zone = Some(any_zone.map_or(temp, |c| c.max(temp)));
            }
        }
        cpu_zone.or(any_zone)
    }
    // macOS SEM leitura de temperatura por ora: o user client do SMC
    // escrito à mão travava DENTRO de uma chamada IOKit no kernel — a thread
    // do worker ficava presa em chamada ininterruptível e o processo virava
    // imortal em TX (nem kill -9 colhia). Quando quisermos temperatura no
    // macOS, adotar um crate consagrado de SMC em vez de FFI artesanal.
    #[cfg(target_os = "macos")]
    {
        None
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

/// This process's resident RAM in MB.
pub fn process_ram_mb() -> Option<f32> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        parse_vm_rss_kb(&status).map(|kb| kb / 1024.0)
    }
    // macOS: proc_pid_rusage (libproc) — UMA chamada de kernel direta,
    // sem fork (o spawn do `ps` era o gatilho dos congelamentos neste
    // beta: a main thread amanhecia dentro do wait4/spawn) e sem IOKit
    // (o SMC artesanal travava o kernel). resident_size = RSS em bytes.
    // Qualquer falha vira None — o segmento sai do painel.
    #[cfg(target_os = "macos")]
    {
        #[repr(C)]
        struct RUsageInfoV2 {
            uuid: [u8; 16],
            user_time: u64,
            system_time: u64,
            pkg_idle_wkups: u64,
            interrupt_wkups: u64,
            pageins: u64,
            wired_fraction: u64,
            resident_fraction: u64,
            resident_size: u64,
            phys_footprint: u64,
        }
        #[link(name = "c")]
        extern "C" {
            fn getpid() -> i32;
            fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut std::ffi::c_void) -> i32;
        }
        const PROC_PID_RUSAGE_V2: i32 = 2;
        let mut info: RUsageInfoV2 = unsafe { std::mem::zeroed() };
        let ok = unsafe {
            proc_pid_rusage(
                getpid(),
                PROC_PID_RUSAGE_V2,
                &mut info as *mut RUsageInfoV2 as *mut std::ffi::c_void,
            )
        } == 0;
        (ok && info.resident_size > 0)
            .then(|| info.resident_size as f32 / 1024.0 / 1024.0)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

/// Millidegrees string ("52000", trailing newline fine) → °C.
#[cfg(target_os = "linux")]
fn parse_thermal_temp(s: &str) -> Option<f32> {
    s.trim().parse::<f32>().ok().map(|milli| milli / 1000.0)
}

/// The `VmRSS:` line ("VmRSS:\t   84352 kB") → kB as f32.
#[cfg(target_os = "linux")]
fn parse_vm_rss_kb(status: &str) -> Option<f32> {
    let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
    line.split_whitespace().nth(1)?.parse::<f32>().ok()
}

#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
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
