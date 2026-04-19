# Velox — Architecture

This document explains the internal structure of the codebase. Read it if you want to modify, extend, or understand why things are the way they are.

---

## Module Map

```
src/
├── main.rs           Entry point, event loop, key handler
├── app.rs            Central application state (App struct)
├── models.rs         Shared data types (ScanResult, ProcessData, SysTickData)
├── scanner/
│   ├── mod.rs        Scannable trait
│   ├── hardware.rs   Disk, RAM, CPU thermals, GPU, power (WMI + sysinfo)
│   ├── software.rs   Registry startup, background services, OS version, drivers
│   ├── usage.rs      Live process enumeration
│   └── worker.rs     Background scanning thread + mpsc channels
└── ui/
    ├── mod.rs        Top-level render() dispatcher
    ├── layout.rs     Screen area subdivision
    └── widgets.rs    Individual widget draw functions
```

---

## Threading Model

Everything that touches WMI, sysinfo, or the Windows Registry runs on one dedicated background thread. The UI event loop runs on the main thread. They communicate through two `std::sync::mpsc` channels:

```
Background Thread                         UI Thread (main)
─────────────────                         ────────────────
COMLibrary (init once)
WMIConnection (init once)
HardwareScanner
UsageScanner
                                          App { hw_rx, sys_rx, ... }
Every 250ms:
  UsageScanner::scan_process_behavior()
  → SysTickData { processes, cpu_pct }
  sys_tx.send(update)  ──────────────────→ app.apply_sys_update(update)

Every 5s:
  HardwareScanner::scan()
  hw_tx.send(hw_data)  ──────────────────→ app.apply_hw_update(hw_data)
```

The UI loop drains both channels with `try_recv()` (non-blocking) at the top of every iteration, then repaints at 50ms. A slow WMI query on the background thread causes the hardware data to be stale for longer — it does not stutter the UI.

The `ScanWorker::spawn()` function in `scanner/worker.rs` encapsulates all of this. It returns a `ScanWorker` with two public `Receiver` fields. The background thread exits cleanly when the receivers are dropped (channel send returns `Err`).

---

## COM / WMI Lifetime

`COMLibrary::new()` calls `CoInitializeEx` for the calling thread. It is called exactly once, at the start of the background thread, not per-scan. The resulting `WMIConnection` is stored as a field of `HardwareScanner` and reused across all scans.

This matters because:
1. COM initialization is thread-local. If you move the scanner to a different thread, you must reinitialize COM on that thread.
2. Creating and destroying `WMIConnection` per-scan has measurable overhead (~50–200ms on some machines) and can leak resources under sustained load.

If WMI initialization fails (e.g., the WMI service is disabled), `wmi_con` is `None` and the thermal/GPU/battery scan methods return empty results with a `warn!` log entry. The rest of the application continues normally.

---

## App State

`App` in `src/app.rs` owns all mutable application state. There are no global variables.

Key fields:

| Field | Type | Purpose |
|-------|------|---------|
| `hardware_data` | `Vec<ScanResult>` | Latest hardware scan, updated every 5s |
| `disk_data` | `Vec<ScanResult>` | Pre-filtered subset of hardware_data (Disk Space entries only) |
| `usage_data` | `Vec<ProcessData>` | Latest process list, updated every 250ms |
| `filtered_usage` | `Vec<ProcessData>` | Search-filtered view of usage_data, rebuilt on change |
| `ram_chart_data` | `Vec<(f64,f64)>` | Pre-computed (x,y) pairs for the RAM chart |
| `cpu_chart_data` | `Vec<(f64,f64)>` | Pre-computed (x,y) pairs for the CPU chart |
| `selected_pid` | `Option<u32>` | Tracks selection by PID, not by row index |
| `kill_confirm_pid` | `Option<u32>` | Set on first `x` press; process is killed on second |

### Why track selected_pid instead of row index?

The process list refreshes every 250ms and is re-sorted on each update. If you tracked the selected row by index, sorting would silently move your selection to a different process. `sync_selection()` is called after every update and re-maps the stored PID to its new row index in `filtered_usage`.

### Pre-computed chart data

`ram_chart_data` and `cpu_chart_data` are `Vec<(f64,f64)>` arrays in the format ratatui's `Chart` widget expects. They are rebuilt in `rebuild_chart_data()` on every 250ms sys tick, not on every 50ms repaint. This eliminates two `Vec` allocations per frame (previously ~200 tuple allocs at 50ms = 4000/sec).

### Pre-filtered disk_data

`disk_data` is a subset of `hardware_data` containing only disk space entries. It is rebuilt in `apply_hw_update()` when hardware data changes (every 5s). The render loop reads it directly instead of re-filtering `hardware_data` on every frame.

---

## Data Flow: Render Pipeline

```
ui::render(&mut App)
  └─ layout::create_main_layout()       → [header, content, footer] Rects
  └─ layout::create_layout(content)     → [disk×N, ram, power, export, processes] Rects
  └─ widgets::draw_header()             ← app.host_name, app.recording, app.paused
  └─ widgets::draw_disk_gauge()         ← app.disk_data[i].raw_value  (no string parse)
  └─ widgets::draw_ram_wave()           ← app.ram_chart_data (pre-computed)
  └─ widgets::draw_power_stats()        ← app.hardware_data (filtered inline, cheap)
  └─ widgets::draw_export_pane()        ← app.export_path, app.export_resolved_path()
  └─ widgets::draw_process_table()      ← app.filtered_usage (pre-filtered)
  └─ widgets::draw_footer()             ← app.kill_confirm_pid, app.paused
```

All data passed to widget functions is already in its final form. No parsing, filtering, or allocation happens inside widget functions (except `draw_process_table`'s `format!` calls per row, which are unavoidable).

---

## ScanResult.raw_value

`ScanResult` carries a `raw_value: Option<f64>` alongside the human-readable `value: String`. Hardware scanners set this to the numeric reading (disk %, RAM %, cap %, etc.) at construction time. Widget functions read `raw_value` directly instead of parsing `value` strings on every frame.

Without this: `draw_disk_gauge` called `"94.3% Full".replace("% Full", "").trim().parse::<f64>()` on every 50ms repaint for every disk. With `raw_value`, it reads a `f64` directly.

---

## Kill Confirmation

Two-step kill is implemented in `App::try_kill_selected()`:

1. First `x` press: sets `kill_confirm_pid = Some(pid)`. Footer shows confirmation prompt.
2. Any key other than `x`/`Delete`: clears `kill_confirm_pid`. The intercepted key is still processed normally.
3. Second `x` press with same PID still armed: calls `kill_process(pid)` which uses `OpenProcess(PROCESS_TERMINATE)` + `TerminateProcess` directly via windows-rs. This avoids allocating a full `sysinfo::System` + `refresh_processes()` just to get a process handle.

Movement keys (`↑`/`↓`) also clear `kill_confirm_pid` via `select_up()`/`select_down()`.

---

## Scanner Trait

```rust
pub trait Scannable {
    fn scan(&mut self) -> Vec<ScanResult>;
}
```

`HardwareScanner` and `SoftwareScanner` implement it. `UsageScanner` does not — it has a different return type (`Vec<ProcessData>`) and a different interface (`scan_process_behavior`, `last_cpu_pct`).

`Scannable` is used for `SoftwareScanner` in the export path in `main.rs`. `HardwareScanner::scan()` is called from the background worker thread where it is owned.

---

## File I/O

All file writes (export snapshot, recording rows) happen on the UI thread. This is acceptable because:
- Export is a one-time user action, not a hot path
- Recording appends one small CSV row every 250ms — well within what a modern disk handles synchronously in microseconds

If you needed to export multi-gigabyte telemetry, move writes to a third channel/thread. For current use cases, the added complexity is not worth it.

---

## Adding a New Hardware Metric

1. Add a new `scan_*` method to `HardwareScanner` in `scanner/hardware.rs`. Return `Vec<ScanResult>`. Set `raw_value` if the metric is numeric.
2. Call the new method from `HardwareScanner::scan()`.
3. The background worker picks it up automatically on the next 5s tick.
4. If you want to display it separately (not in the generic Power/Hardware Stats list), add a new `ActivePane` variant and a new widget function.

The hardware stats list in `draw_power_stats` shows everything in `hardware_data` that is not a disk or RAM entry. New metrics appear there automatically.

---

## What Is Not Here

- **No config file.** Scan intervals (5s hardware, 250ms sys) are constants in `scanner/worker.rs`. Export path defaults to `./report.txt`. If you want persistence, write a TOML config and load it in `App::new()`.
- **No network.** Nothing phones home. No metrics are sent anywhere except to local files you explicitly trigger.
- **No installer.** Single binary, no DLL dependencies beyond the Windows system libraries. Copy it, run it.
- **No tests.** The scanning functions are tightly coupled to Windows APIs and live system state, which makes unit testing impractical without mocking. Integration testing on CI would need a Windows runner with WMI enabled.
