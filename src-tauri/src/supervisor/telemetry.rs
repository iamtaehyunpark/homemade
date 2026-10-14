//! 1s poller: sysinfo CPU/RAM, per-run RSS (sum over process group), GPU memory. OWNER: agent C.
//! Linux: nvml-wrapper (memory_info + running_compute_processes mapped to run PIDs/descendants).
//! macOS: gpu_total = 75% of hw.memsize; gpu_used = "In use system memory" parsed from
//!   `ioreg -r -d 1 -w 0 -c IOAccelerator` (PerformanceStatistics); gpu_label = "GPU memory headroom".
//! Each tick: update ActiveRun.peak_ram_mb/peak_vram_mb (AtomicU64 fetch_max), write state.resources,
//! emit "resources://tick" ResourceSnapshot (include queue from state.vram_queue), then scheduler::on_run_finished(state).
use crate::state::AppState;
use crate::types::{QueuedJobInfo, ResourceSnapshot, RunResourceUsage};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use sysinfo::{ProcessesToUpdate, System};

const MIB: u64 = 1024 * 1024;

pub fn start(state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let mut sys = System::new();
        let mut gpu = GpuProbe::new();
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            sys.refresh_processes(ProcessesToUpdate::All, true);

            // pid -> parent map, used to attribute descendants to runs.
            let parents: HashMap<u32, u32> = sys.processes().iter()
                .filter_map(|(pid, p)| p.parent().map(|pp| (pid.as_u32(), pp.as_u32()))).collect();
            let run_pids: HashMap<u32, String> = state.active_runs.iter().map(|r| (r.pid as u32, r.run_id.clone())).collect();
            let owner = |pid: u32| -> Option<String> {
                let mut cur = pid;
                for _ in 0..=8 {
                    if let Some(r) = run_pids.get(&cur) { return Some(r.clone()); }
                    cur = *parents.get(&cur)?;
                }
                None
            };

            let mut usage: HashMap<String, (f32, u64)> = HashMap::new();
            if !run_pids.is_empty() {
                for (pid, p) in sys.processes() {
                    if let Some(run) = owner(pid.as_u32()) {
                        let e = usage.entry(run).or_default();
                        e.0 += p.cpu_usage();
                        e.1 += p.memory();
                    }
                }
            }
            let (gpu_used_mb, gpu_total_mb, gpu_label, gpu_pids) = gpu.poll();
            let mut vram: HashMap<String, u64> = HashMap::new();
            for (pid, mb) in gpu_pids {
                if let Some(run) = owner(pid) { *vram.entry(run).or_default() += mb; }
            }

            let mut per_run = Vec::new();
            for r in state.active_runs.iter() {
                let (cpu, bytes) = usage.get(&r.run_id).copied().unwrap_or_default();
                let ram_mb = bytes / MIB;
                let vram_mb = vram.get(&r.run_id).copied().unwrap_or(0);
                r.peak_ram_mb.fetch_max(ram_mb, Ordering::Relaxed);
                r.peak_vram_mb.fetch_max(vram_mb, Ordering::Relaxed);
                per_run.push(RunResourceUsage { run_id: r.run_id.clone(), app_id: r.app_id.clone(), cpu_pct: cpu, ram_mb, vram_mb });
            }

            let queue = match state.vram_queue.try_lock() {
                Ok(q) => Some(q.iter().map(|j| QueuedJobInfo {
                    run_id: j.run_id.clone(), app_id: j.app_id.clone(),
                    required_vram_mb: if j.required_vram_mb > 0 { j.required_vram_mb } else { 2048 },
                    reason: "Waiting for GPU memory".into(),
                }).collect::<Vec<_>>()),
                Err(_) => None,
            };

            let snap = {
                let mut res = state.resources.lock().unwrap();
                let queue = queue.unwrap_or_else(|| res.queue.clone());
                *res = ResourceSnapshot {
                    cpu_pct: sys.global_cpu_usage(),
                    ram_used_mb: sys.used_memory() / MIB,
                    ram_total_mb: sys.total_memory() / MIB,
                    gpu_used_mb, gpu_total_mb, gpu_label, per_run, queue,
                };
                res.clone()
            };
            state.emit("resources://tick", snap);
            super::scheduler::on_run_finished(state.clone()).await;
        }
    });
}

/// Returns (used_mb, total_mb, label, [(pid, vram_mb)]).
type GpuReading = (u64, u64, String, Vec<(u32, u64)>);

#[cfg(target_os = "macos")]
struct GpuProbe { total_mb: u64 }
#[cfg(target_os = "macos")]
impl GpuProbe {
    fn new() -> Self {
        let memsize = std::process::Command::new("sysctl").args(["-n", "hw.memsize"]).output().ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<u64>().ok()).unwrap_or(0);
        Self { total_mb: (memsize as f64 * 0.75 / MIB as f64) as u64 }
    }
    fn poll(&mut self) -> GpuReading {
        let used = std::process::Command::new("ioreg").args(["-r", "-d", "1", "-w", "0", "-c", "IOAccelerator"]).output().ok()
            .map(|o| parse_in_use(&String::from_utf8_lossy(&o.stdout))).unwrap_or(0);
        (used / MIB, self.total_mb, "GPU memory headroom".into(), vec![])
    }
}

/// Sum every `"In use system memory"=<n>` occurrence (bytes).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_in_use(s: &str) -> u64 {
    const KEY: &str = "\"In use system memory\"=";
    s.match_indices(KEY).filter_map(|(i, _)| {
        let rest = &s[i + KEY.len()..];
        let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        rest[..end].parse::<u64>().ok()
    }).sum()
}

#[cfg(target_os = "linux")]
struct GpuProbe { nvml: Option<nvml_wrapper::Nvml> }
#[cfg(target_os = "linux")]
impl GpuProbe {
    fn new() -> Self { Self { nvml: nvml_wrapper::Nvml::init().ok() } }
    fn poll(&mut self) -> GpuReading {
        let Some(nvml) = &self.nvml else { return (0, 0, "VRAM".into(), vec![]) };
        let Ok(dev) = nvml.device_by_index(0) else { return (0, 0, "VRAM".into(), vec![]) };
        let (used, total) = dev.memory_info().map(|m| (m.used / MIB, m.total / MIB)).unwrap_or((0, 0));
        let procs = dev.running_compute_processes().unwrap_or_default().into_iter().map(|p| {
            let mb = match p.used_gpu_memory { nvml_wrapper::enums::device::UsedGpuMemory::Used(b) => b / MIB, _ => 0 };
            (p.pid, mb)
        }).collect();
        (used, total, "VRAM".into(), procs)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
struct GpuProbe;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
impl GpuProbe {
    fn new() -> Self { Self }
    fn poll(&mut self) -> GpuReading { (0, 0, "VRAM".into(), vec![]) }
}
