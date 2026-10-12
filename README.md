# cosmic-cpufreq

A COSMIC applet for inspecting and changing CPU frequency scaling settings on Linux.

> **Note**
>
> This is a fork of [skylord123/cosmic-cpufreq](https://github.com/skylord123/cosmic-cpufreq).
> The changes in this fork were developed with the help of AI.

Features:
- show current average CPU frequency in the panel (icon and/or text)
- toggle turbo boost
- change CPU governor
- change energy performance preference when available
- adjust minimum and maximum scaling frequency
- tabbed popup: a clean Controls view plus a Details view for system info, per-core frequencies and usage bars
- selectable popup layout: tabbed, single horizontal (info beside controls) or single vertical (info above controls)
- configurable panel display (average/min/max), frequency unit and decimal precision
- optional CPU temperature and CPU usage percentage on the panel button, with a configurable separator character (or none) between the panel values
- CPU temperature read from hwmon: a package-level bar plus optional per-core temperature shown inline in the core grid, panel display, and °C/°F selection
- system information shown in a System section in the settings page
- per-block visibility toggles for the per-core grid, CPU and memory usage
- configurable per-core grid columns and separate min/max frequency slider toggles
- restore saved turbo/governor settings on launch (toggleable)

![cosmic-cpufreq-image](img.png)

## Requirements

- COSMIC desktop with applet support
- Rust toolchain
- `just`
- `pkexec` / polkit agent available in the desktop session
- kernel CPU frequency interfaces exposed under `/sys/devices/system/cpu`

## Build

```bash
cargo build --release
```

## Install

Install the user-local files:

```bash
just install
```

This places:
- the applet at `~/.local/bin/cosmic-ext-applet-cpufreq`
- the helper at `~/.local/bin/cosmic-cpufreqctl`
- the policy template at `~/.local/bin/dev.skylar.cosmic-ext-applet-cpufreq.policy`
- the desktop file at `~/.local/share/applications/dev.skylar.cosmic-ext-applet-cpufreq.desktop`

Then start or reload the applet from COSMIC. The first time you change a privileged setting, you should get a polkit prompt so the helper and policy can be installed system-wide.

## Updating after code changes

When you rebuild the applet, reinstall the local files:

```bash
cargo build --release
just install
```

If the applet is already running, reload it in COSMIC so the new binary is actually used. A running process can otherwise continue using an older deleted binary image.

If you need to force a clean helper/policy reinstall, remove the system copies:

```bash
pkexec /usr/bin/cosmic-cpufreqctl uninstall
```

Then change a setting in the applet again to trigger reinstallation of the current helper and policy.

## Frequency bounds

The applet uses kernel-exposed limits for writable frequency bounds:
- minimum comes from `cpuinfo_min_freq`
- maximum comes from `cpuinfo_max_freq`
- on Intel `intel_pstate`, turbo disabled uses `base_frequency` as the effective writable ceiling

This is intentional: slider limits should match frequencies the kernel will actually accept when writing `scaling_min_freq` and `scaling_max_freq`.

## Notes

- Governor and EPP controls only appear when the corresponding sysfs interfaces are available.
- CPU temperature is read from `/sys/class/hwmon` (`coretemp`, `k10temp`, `zenpower`, `cpu_thermal`), preferring a package-level sensor.
- Frequency sliders update on release, not continuously while dragging.
- The panel text shows the current CPU frequency (average by default; minimum/maximum selectable).

## Uninstall

Remove the local files:

```bash
just uninstall
```

Remove the system helper and polkit policy:

```bash
pkexec /usr/bin/cosmic-cpufreqctl uninstall
```
