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
        let mut cpu_zone: Option<f32> = None;
        let mut any_zone: Option<f32> = None;
        let zones = std::fs::read_dir("/sys/class/thermal").ok()?;
        for zone in zones.flatten() {
            let dir = zone.path();
            let temp = parse_thermal_temp(&std::fs::read_to_string(dir.join("temp")).ok()?)?;
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
    #[cfg(target_os = "macos")]
    {
        mac::smc_cpu_temp()
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
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .ok()?;
        out.status.success().then_some(())?;
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse::<f32>()
            .ok()
            .map(|kb| kb / 1024.0)
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

// ---------------------------------------------------------------------------
// macOS — SMC user client (IOKit). Toda chamada checa o kern_return_t e o
// caminho limpa os handles: qualquer falha vira `None` e o segmento sai do
// painel. As chaves candidatas cobrem Intel (TCxx) e Apple Silicon (Tpxx) —
// a que não existe no modelo responde kSMCKeyNotFound e é pulada.
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod mac {
    use std::os::raw::c_void;

    type IOReturn = i32;
    #[allow(non_camel_case_types)]
    type mach_port_t = u32;
    type io_object_t = u32;

    const KERN_SUCCESS: IOReturn = 0;
    const SANE_TEMP: std::ops::RangeInclusive<f32> = 10.0..=110.0;

    // Chaves de temperatura usuais: Intel primeiro, depois as do Apple
    // Silicon (nomes variam por geração — a que não existe no modelo
    // responde kSMCKeyNotFound e é pulada).
    const TEMP_KEYS: &[&[u8; 4]] = &[
        b"TC0P", b"TC0E", b"TCAD", b"TC0D", b"Tp01", b"Tp05", b"Tp09", b"Tp0C", b"Tp0D", b"Tp1P",
    ];

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct SMCVersion([u8; 5]);

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct SMCKeyInfoData {
        data_size: u8,
        data_type: u32,
        data_attributes: u8,
    }

    #[repr(C)]
    struct SMCParamStruct {
        key: u32,
        vers: SMCVersion,
        p_limit_data: u16,
        key_info: SMCKeyInfoData,
        result: u8,
        status: u8,
        data8: u8,
        data32: u32,
        bytes: [u8; 32],
    }

    impl SMCParamStruct {
        fn zeroed() -> Self {
            SMCParamStruct {
                key: 0,
                vers: SMCVersion([0; 5]),
                p_limit_data: 0,
                key_info: SMCKeyInfoData {
                    data_size: 0,
                    data_type: 0,
                    data_attributes: 0,
                },
                result: 0,
                status: 0,
                data8: 0,
                data32: 0,
                bytes: [0; 32],
            }
        }

        fn set_key(&mut self, key: &[u8; 4]) {
            self.key = u32::from_be_bytes(*key);
        }
    }

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOMasterPort(bootstrap: mach_port_t, master: *mut mach_port_t) -> IOReturn;
        fn IOServiceGetMatchingService(master: mach_port_t, matching: *mut c_void)
            -> io_object_t;
        fn IOServiceMatching(name: *const std::os::raw::c_char) -> *mut c_void;
        fn IOServiceOpen(
            service: io_object_t,
            owning_task: mach_port_t,
            kind: u32,
            connect: *mut mach_port_t,
        ) -> IOReturn;
        fn IOServiceClose(service: io_object_t) -> IOReturn;
        fn IOObjectRelease(obj: io_object_t) -> IOReturn;
        fn IOConnectCallStructMethod(
            connection: mach_port_t,
            selector: u32,
            input: *const c_void,
            input_size: usize,
            output: *mut c_void,
            output_size: *mut usize,
        ) -> IOReturn;
    }

    extern "C" {
        static mach_task_self_: mach_port_t;
    }

    const SEL_OPEN: u32 = 0;
    const SEL_CLOSE: u32 = 1;
    const SEL_READ_KEY: u32 = 5;
    const SEL_GET_KEY_INFO: u32 = 9;

    /// Guard that always closes the connection and releases the service.
    struct Smc {
        connect: mach_port_t,
        service: io_object_t,
    }

    impl Smc {
        fn open() -> Option<Self> {
            unsafe {
                let mut master: mach_port_t = 0;
                if IOMasterPort(0, &mut master) != KERN_SUCCESS {
                    return None;
                }
                let name = b"AppleSMC\0";
                let matching = IOServiceMatching(name.as_ptr() as *const _);
                if matching.is_null() {
                    return None;
                }
                let service = IOServiceGetMatchingService(master, matching);
                if service == 0 {
                    return None;
                }
                let mut connect: mach_port_t = 0;
                if IOServiceOpen(service, mach_task_self_, 0, &mut connect) != KERN_SUCCESS {
                    IOObjectRelease(service);
                    return None;
                }
                Some(Smc { connect, service })
            }
        }

        /// One struct-method call; returns the output struct on success.
        fn call(&self, selector: u32, input: &SMCParamStruct) -> Option<SMCParamStruct> {
            unsafe {
                let mut output = SMCParamStruct::zeroed();
                let mut size = std::mem::size_of::<SMCParamStruct>();
                let kern = IOConnectCallStructMethod(
                    self.connect,
                    selector,
                    input as *const SMCParamStruct as *const c_void,
                    std::mem::size_of::<SMCParamStruct>(),
                    &mut output as *mut SMCParamStruct as *mut c_void,
                    &mut size,
                );
                (kern == KERN_SUCCESS).then_some(output)
            }
        }

        fn read_key(&self, key: &[u8; 4]) -> Option<SMCParamStruct> {
            // 1) info da chave (tamanho/tipo), 2) os bytes em si.
            let mut input = SMCParamStruct::zeroed();
            input.set_key(key);
            let info = self.call(SEL_GET_KEY_INFO, &input)?;
            if info.result != 0 {
                return None;
            }
            input.key_info.data_size = info.key_info.data_size;
            let out = self.call(SEL_READ_KEY, &input)?;
            if out.result != 0 {
                return None;
            }
            Some(out)
        }

        /// Valor de temperatura de um key já lido — sp78 (fixpoint) ou flt.
        fn temp_from(out: &SMCParamStruct) -> Option<f32> {
            let data_type = out.key_info.data_type.to_be_bytes();
            let temp = if data_type == *b"sp78" && out.key_info.data_size >= 2 {
                Some(i16::from_be_bytes([out.bytes[0], out.bytes[1]]) as f32 / 256.0)
            } else if &data_type[..3] == b"flt" && out.key_info.data_size >= 4 {
                Some(f32::from_be_bytes([
                    out.bytes[0],
                    out.bytes[1],
                    out.bytes[2],
                    out.bytes[3],
                ]))
            } else if &data_type[..3] == b"ui8" && out.key_info.data_size >= 1 {
                Some(out.bytes[0] as f32)
            } else {
                None
            };
            temp.filter(|t| SANE_TEMP.contains(t))
        }
    }

    impl Drop for Smc {
        fn drop(&mut self) {
            unsafe {
                let mut out = SMCParamStruct::zeroed();
                let mut size = std::mem::size_of::<SMCParamStruct>();
                let _ = IOConnectCallStructMethod(
                    self.connect,
                    SEL_CLOSE,
                    std::ptr::null(),
                    0,
                    &mut out as *mut SMCParamStruct as *mut c_void,
                    &mut size,
                );
                IOServiceClose(self.connect);
                IOObjectRelease(self.service);
            }
        }
    }

    pub fn smc_cpu_temp() -> Option<f32> {
        let smc = Smc::open()?;
        for key in TEMP_KEYS {
            if let Some(out) = smc.read_key(key) {
                if let Some(t) = Smc::temp_from(&out) {
                    return Some(t);
                }
            }
        }
        None
    }
}
