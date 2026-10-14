//! System readouts for the dev-mode perf overlay: CPU temperature and this
//! process's resident RAM. Every source is best-effort `Option` — the
//! overlay just omits what the OS won't tell. Parsing is split into pure
//! functions so the tests can pin the formats without a live kernel.
//!
//! - Linux: thermal zones of sysfs + `VmRSS` de /proc/self/status.
//! - macOS: RAM via proc_pid_rusage (libproc, worker de fundo — NENHUMA
//!   syscall na thread de UI: IOKit e fork engasgavam o kernel deste
//!   beta). Temperatura sem leitura por ora (SMC artesanal travava).

/// CPU temperature in °C. Linux prefers zones whose `type` mentions
/// cpu/soc (handhelds expose several — battery, GPU, ACPI noise), falling
/// back to the hottest zone; macOS probes the usual SMC keys.
#[cfg(target_os = "macos")]
mod mac {
    use std::os::raw::c_void;

    /// RAM residente do processo, MB — proc_pid_rusage (libproc), chamada
    /// direta de kernel, sem fork e sem IOKit.
    pub fn proc_rss_mb() -> Option<f32> {
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
            fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut c_void) -> i32;
        }
        const PROC_PID_RUSAGE_V2: i32 = 2;
        let mut info: RUsageInfoV2 = unsafe { std::mem::zeroed() };
        let ok = unsafe {
            proc_pid_rusage(
                getpid(),
                PROC_PID_RUSAGE_V2,
                &mut info as *mut RUsageInfoV2 as *mut c_void,
            )
        } == 0;
        (ok && info.resident_size > 0).then(|| info.resident_size as f32 / 1024.0 / 1024.0)
    }
}

/// CPU temperature in °C.
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
    // macOS: MESMA disciplina de sempre — NENHUMA leitura na thread de
    // UI. Neste beta do macOS cada syscall que o monitoramento tentou da
    // main (IOKit, fork+ps, proc_info) acabou wedging no kernel. O worker
    // de fundo lê a cada 2 s (proc_pid_rusage — uma chamada direta de
    // libproc, sem fork, sem IOKit) e publica num atomic; a UI só lê o
    // cache. Se a leitura segurar no kernel, o app NÃO sente: o pior caso
    // é o segmento congelar no último valor ou ausentar.
    #[cfg(target_os = "macos")]
    {
        use std::sync::atomic::{AtomicI32, Ordering};
        static CACHE: AtomicI32 = AtomicI32::new(i32::MIN);
        static STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        STARTED.get_or_init(|| {
            let _ = std::thread::Builder::new()
                .name("ram-readout".into())
                .spawn(|| loop {
                    let value = match mac::proc_rss_mb() {
                        Some(mb) => (mb * 10.0) as i32,
                        None => -1,
                    };
                    CACHE.store(value, Ordering::Relaxed);
                    std::thread::sleep(std::time::Duration::from_secs(2));
                });
        });
        match CACHE.load(Ordering::Relaxed) {
            i32::MIN => None,
            -1 => None,
            tenths => Some(tenths as f32 / 10.0),
        }
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
