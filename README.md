# Velox

A Windows system diagnostics terminal UI. It queries hardware state, running processes, and software configuration in real time and displays everything in an interactive TUI built with Ratatui.

This is not a toy. It reads WMI, the Windows Registry, PnP device nodes, and power scheme settings directly. It kills processes via `TerminateProcess`. It records telemetry to CSV. If you don't know what WMI is, you probably want something else.

![Velox screenshot](logo/logo.png)

---

## What It Does

- **Hardware**: Disk usage per volume, RAM pressure (including pagefile thrash detection), CPU thermal state and throttle ratio via WMI `Win32_Processor`, GPU VRAM via `Win32_VideoController`, battery status, CPU performance cap from the active power scheme
- **Processes**: Live process table sorted by CPU or memory. Top 150 processes. Search by name or PID. Kill any process with two-step confirmation
- **Software**: Startup registry entries from `HKCU` and `HKLM` Run keys, active background services (Defender, Search Indexer), OS version from Registry (not the WMI lie), PnP driver health via `CM_Get_DevNode_Status`
- **Export**: Snapshot report to a text file. Continuous telemetry recording to CSV at 250ms intervals

All scanning runs on a dedicated background thread. The UI thread never blocks on WMI queries or sysinfo refreshes.

---

## Requirements

- **OS**: Windows 10 or Windows 11 (x86-64). This will not compile or run on Linux or macOS — it uses `windows-rs` bindings and WMI directly.
- **Rust**: 1.70 or later (uses `let-else` chains)
- **Terminal**: Any terminal that supports 256-color ANSI and Braille Unicode (Windows Terminal recommended; legacy cmd.exe will render poorly)
- **Permissions**: Standard user is sufficient for most queries. Killing system processes may require elevation

---

## Building

```
cargo build --release
```

The binary lands at `target/release/velox.exe`.

Debug builds (`cargo build`) work but WMI queries are noticeably slower without optimizations.

To see internal warnings (WMI failures, registry misses, kill errors):

```
set RUST_LOG=warn
velox.exe
```

The default log level is `warn` even without `RUST_LOG` set.

---

## Running

```
velox.exe
```

There are no command-line arguments. Configuration is done at runtime via the TUI.

---

## Key Bindings

| Key | Action |
|-----|--------|
| `Tab` | Cycle active pane: Disk → RAM → Power → Export → Processes |
| `Enter` | Zoom selected pane to full screen |
| `Esc` / `q` | Exit zoom / quit |
| `p` | Pause/resume process refresh (freezes table for reading) |
| `r` | Toggle continuous recording to CSV |
| `i` | Enter input mode (search in Processes pane, edit path in Export pane) |
| `s` | Toggle sort: CPU% ↔ Memory |
| `x` / `Delete` | Kill selected process (press twice to confirm) |
| `↑` / `k` | Scroll up in process table |
| `↓` / `j` | Scroll down in process table |

---

## Export

Two modes controlled by `r`:

**Snapshot** (default): Press `Enter` in the Export pane. Writes a categorized text report including hardware state, software scan, and top 25 processes by CPU to the configured path.

**Recording**: Press `r` to toggle. Creates (or truncates) a CSV at the configured path with header:
```
Timestamp,CPU_Global%,RAM_Used%,Top_Process_Name,Top_Process_CPU%,Top_Process_MemMB
```
Appends one row per 250ms tick until `r` is pressed again.

Default path is `./report.txt` (relative to the working directory where the binary was launched). The export pane shows the resolved absolute path so there is no ambiguity about where the file lands.

---

## Dependencies

| Crate | Why |
|-------|-----|
| `sysinfo 0.30` | Process enumeration, RAM/CPU/disk stats via NT kernel APIs |
| `wmi 0.13` | WMI queries for CPU thermals, GPU VRAM, battery |
| `windows 0.52` | Direct Win32 bindings: Registry enumeration, PnP device tree, `TerminateProcess` |
| `winreg 0.52` | High-level registry reads for OS version strings |
| `ratatui 0.26` | Terminal UI layout, widgets, Braille chart rendering |
| `crossterm 0.27` | Cross-platform raw terminal I/O and keyboard events |
| `serde` + `serde_json` | WMI query result deserialization |
| `chrono` | Timestamps in CSV recording |
| `log` + `env_logger` | Internal diagnostic logging |
| `anyhow` | Error context propagation |
| `strum` / `strum_macros` | Enum utilities |

`strum` and `anyhow` are present but not heavily exercised in the current codebase. They are left in for future error propagation work.

---

## Known Limitations

- WMI `Win32_Battery` returns no data on desktop machines (no battery). This is expected; the Power Source entry simply won't appear in the Hardware Stats pane.
- GPU VRAM from `Win32_VideoController.AdapterRAM` is a 32-bit field, which means it saturates at ~4 GB for high-VRAM cards. This is a WMI limitation, not a bug in this code.
- CPU thermal data comes from `Win32_Processor.LoadPercentage`, not hardware temperature sensors. If you want Celsius readings, you need a kernel driver or `OpenHardwareMonitor`.
- The PnP driver health check scans the root device node only. It will detect root-level failures but not every faulty child device.
- Process list is capped at 150 entries (top by CPU). Idle/zero-memory processes are excluded.
- No config file. Export path and scan intervals are not persisted between sessions.

---

## License

MIT. See `LICENSE`.
