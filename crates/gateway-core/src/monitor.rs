//! Host metrics via [sysinfo](https://github.com/GuillaumeGomez/sysinfo). Do not parse /proc by hand.

use gateway_model::{DiskMetric, HostMetrics, NetMetric};
use std::time::Duration;
use sysinfo::{Disks, Networks, System};

pub fn sample_host() -> HostMetrics {
    let mut sys = System::new();
    sys.refresh_memory();
    sys.refresh_cpu_all();
    std::thread::sleep(Duration::from_millis(150));
    sys.refresh_cpu_all();
    sys.refresh_memory();

    let load = System::load_average();
    let disks = Disks::new_with_refreshed_list();
    let nets = Networks::new_with_refreshed_list();

    HostMetrics {
        hostname: System::host_name().unwrap_or_else(|| "unknown".into()),
        os: System::long_os_version()
            .or_else(System::name)
            .unwrap_or_else(|| "unknown".into()),
        kernel: System::kernel_version().unwrap_or_else(|| "unknown".into()),
        uptime_secs: System::uptime(),
        cpu_percent: sys.global_cpu_usage(),
        cpu_count: sys.cpus().len(),
        load_1: load.one,
        load_5: load.five,
        load_15: load.fifteen,
        mem_total_bytes: sys.total_memory(),
        mem_used_bytes: sys.used_memory(),
        disks: disks
            .iter()
            .filter(|d| d.total_space() > 0)
            .map(|d| {
                let total = d.total_space();
                let avail = d.available_space();
                DiskMetric {
                    mount: d.mount_point().to_string_lossy().into_owned(),
                    total_bytes: total,
                    used_bytes: total.saturating_sub(avail),
                }
            })
            .collect(),
        nets: nets
            .iter()
            .filter(|(name, _)| {
                let n = name.to_ascii_lowercase();
                !n.starts_with("lo") && !n.contains("loopback")
            })
            .map(|(name, data)| NetMetric {
                name: name.clone(),
                rx_bytes: data.total_received(),
                tx_bytes: data.total_transmitted(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::sample_host;

    #[test]
    fn host_sample_has_memory() {
        let m = sample_host();
        assert!(m.mem_total_bytes > 0);
        assert!(!m.hostname.is_empty());
    }
}
