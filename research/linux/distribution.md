# Linux variant: packaging, permissions, CI/CD

Research for the Linux port of Open X3D Pro (Logitech Extreme 3D Pro, USB `046d:c215`). Covers both candidate
architectures: **(U)** a userspace daemon that grabs the evdev node and re-emits through `/dev/uinput`, and **(B)** a
HID-BPF program attached at plug time with runtime config in a BPF map. Sources were read on 2026-10-04 and are listed
at the end as [S#]. "Verified" means read in a primary source or in the systemd 257 / kernel 6.12 files on a Debian 13
host. Anything else is marked **verify**.

## 0. Recommendations

1. **Permissions (U):** ship one udev rule with `TAG+="uaccess"` for `/dev/uinput` (the same line Valve ships in
   `steam-devices`) and for the stick's hidraw node, and run the remapper as a **`systemd --user` service**. systemd already
   tags every joystick evdev node `uaccess`, so the stick's node needs no rule. There's no root daemon, no `input` group
   and no logout: the ACL is applied to the active seat user as soon as the rule is triggered.
2. **Permissions (B):** loading a HID-BPF program needs CAP_BPF and CAP_PERFMON, so it is never available to a desktop
   user. Ship our own loader as a **hardened system service**, started by udev `SYSTEMD_WANTS` when the stick is plugged
   in. It holds the link and the map fds and takes config over a socket-activated AF_UNIX socket. We should not depend
   on `udev-hid-bpf` for distribution.
3. **Packaging:** **deb + rpm + AppImage** from Tauri, with Linux-only config in `app/src-tauri/tauri.linux.conf.json`.
   deb and rpm install the rule to `/usr/lib/udev/rules.d/` and reload udev in postinst. The AppImage offers a one-time
   `pkexec` "install permissions" step that writes to `/etc/udev/rules.d/`. **No Flatpak** and **no Snap** for v1.
   Publish an AUR `-bin` package that reuses the `.deb`.
4. **SteamOS:** use the AppImage plus the pkexec helper, with the rule in `/etc/udev/rules.d/`. The helper must also
   write `/etc/atomic-update.conf.d/openx3d.conf`, or SteamOS ≥3.6 drops the rule on the next OS update [S20].
5. **CI:** add a `linux` job to `build.yml`. It runs on **ubuntu-22.04** for glibc 2.35, so Debian 12 is covered.
   Add a `linux-e2e` lane on ubuntu-24.04 that drives the daemon through a fake stick made with `/dev/uhid`. If (B) is
   chosen, add a `virtme-ng` lane that boots a kernel built with `CONFIG_HID_BPF=y` under KVM. KVM is available on
   standard runners for public repos [S27].
6. **Tier 1:** Ubuntu 24.04/26.04, Fedora 43/44, Arch, SteamOS 3.8. Path (B) is gated by kernel ≥ 6.11 **and**
   `CONFIG_HID_BPF=y`. The **Debian 13 stock kernel has HID-BPF disabled** [S16], so (B) cannot be the only path.

## 1. Permission model

### 1.1 What each operation needs (default state verified on systemd 257, kernel 6.12)

| Op | Kernel check | Default on systemd distros | What we need |
|---|---|---|---|
| (a) open stick evdev `/dev/input/eventN` | DAC/ACL on node | `50-udev-default.rules`: `SUBSYSTEM=="input", GROUP="input"` (mode 0660). `70-uaccess.rules`: `SUBSYSTEM=="input", ENV{ID_INPUT_JOYSTICK}=="?*", TAG+="uaccess"` gives the **active seat user rw ACL** [S1]. The rule has existed since at least systemd v245 [S1] | **Nothing.** Requires that `input_id` classifies the stick as a joystick (it reports BTN_TRIGGER; **verify** with `udevadm info`) |
| (b) `EVIOCGRAB` | ioctl on an open fd, no extra capability | follows (a) | nothing beyond (a). A grab blocks other evdev handlers including joydev `js*`, but not hidraw readers |
| (c) create a uinput device | rw on `/dev/uinput` (misc 10:223) | **root 0600.** No upstream systemd rule tags it [S1] | our rule `KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess", OPTIONS+="static_node=uinput"`, identical to Valve's `60-steam-input.rules` [S3] |
| (d) read hidraw | DAC/ACL | root 0600. uaccess only for specific classes (3D mice, wallets, AV controllers) [S1] | our rule matching `ATTRS{idVendor}=="046d", ATTRS{idProduct}=="c215"`. Only needed if the GUI reads raw reports, because evdev already carries every axis |
| (e) load HID-BPF | `BPF_PROG_TYPE_STRUCT_OPS` is a perfmon prog type and needs **CAP_BPF + CAP_PERFMON**. A `STRUCT_OPS` map needs CAP_BPF [S11]. The checks apply whatever `kernel.unprivileged_bpf_disabled` is set to. CAP_SYS_ADMIN implies both | n/a | root, or a service holding those two capabilities. udev `RUN` runs as root but processes are **killed when the event finishes** [S2], so a RUN loader must pin the link. `udev-hid-bpf` does exactly that |
| (e') update the BPF map at runtime | since **5.19** only `BPF_PROG_LOAD` and `BPF_MAP_CREATE` are gated by `unprivileged_bpf_disabled`. `BPF_OBJ_GET` on a pinned map is checked like a file (`path_permission(ACC_MODE)`) [S12][S13] | systemd mounts `/sys/fs/bpf` with **`mode=0700`** [S14]. Pins are created `0600 & ~umask` [S13] | a non-root writer would need a separate bpffs mount (`mode=`, `gid=` options exist [S13]) plus chown on the pin. **Recommended instead:** expose no pin and route updates through the loader daemon (§3.3) |

`uaccess` mechanics, verified in `73-seat-late.rules`: the `uaccess` builtin runs in `73-seat-late.rules` for devices
tagged earlier, so **our file must sort before 73**, hence `70-openx3d.rules`. `static_node=` applies the permissions
and tags to the node that `kmod static-nodes` creates before the module loads [S2]. ACLs follow the *active* session:
fast user switching moves them, SSH or inactive sessions get none, and an fd that is already open stays usable.

### 1.2 Options compared

| Option | Covers | Immediate? | Security | Verdict |
|---|---|---|---|---|
| **udev `TAG+="uaccess"`** (logind seat ACL) | a–d | yes, after `udevadm trigger` | only the active local user. uinput uaccess lets any process of that user inject input, the same exposure Steam already accepts on most gaming desktops [S3] | **default for (U)** |
| `input` group | a, b, (c only where distro sets `/dev/uinput` to group input) | **no**, needs re-login | gives **read access to every keyboard and mouse** (a system-wide keylogger) and grab of all of them. The member can also hijack another seat | fallback for headless or non-logind systems only. Document it, never automate it |
| dedicated group, e.g. `uinput` (NixOS `hardware.uinput` does this [S7]) | c | no, re-login | narrower than `input` | alternative for SSH or headless installs |
| polkit + privileged helper (`pkexec` / D-Bus) | install-time work and one-shot privileged actions | yes (password prompt) | admin auth per action (`org.freedesktop.policykit.exec`) [S21] | use only for **AppImage/SteamOS rule install** |
| root system daemon + socket (`openx3d.socket` → root service doing uinput) | a–e | yes | a root process parses user config, and the daemon must find the active user for per-app profiles. This is input-remapper's model: root D-Bus service, polkit `auth_admin_keep`, udev `RUN` autoload [S4][S5] | **only for (B)**, where root is unavoidable |
| `systemd --user` service | runs (U) as the user | yes | inherits uaccess ACLs. It also runs in SteamOS Game Mode, where XDG autostart does not. Cgroup device filters (`DeviceAllow=`) need privileges the user manager lacks (**verify**) | **runtime for (U)** |
| hardening: `DeviceAllow=` / `DevicePolicy=closed`, `SupplementaryGroups=`, capability bounding | system units | n/a | `DeviceAllow` uses eBPF and only restricts, it never grants DAC access. Groups such as `char-input` must be resolvable when the unit starts; pair with `Wants=/After=modprobe@uinput.service` [S9] | apply to the (B) service |

Other projects: keyd runs as a root system service with a `keyd` group for its IPC socket [S6]. SC-Controller ships the
uinput rule with `MODE="0666"` [S8], which is world-writable and should not be copied.

### 1.3 Rule file (deb/rpm: `/usr/lib/udev/rules.d/70-openx3d.rules`; AppImage/SteamOS: `/etc/udev/rules.d/`)

```udev
# Open X3D Pro - Logitech Extreme 3D Pro (046d:c215). Must sort before 73-seat-late.rules (uaccess builtin).
# The stick's evdev node is already uaccess via systemd 70-uaccess.rules (ID_INPUT_JOYSTICK).

# Virtual output device; same rule as Valve's steam-devices 60-steam-input.rules.
KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess", OPTIONS+="static_node=uinput"

# Raw HID reports of this stick only (GUI live view / diagnostics).
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="046d", ATTRS{idProduct}=="c215", TAG+="uaccess"

# (B) only: start the HID-BPF loader when the stick appears. HID_ID format: verify with udevadm info.
SUBSYSTEM=="hid", ENV{HID_ID}=="0003:0000046D:0000C215", TAG+="systemd", ENV{SYSTEMD_WANTS}+="openx3d-bpf.service"
```

Package files go under `/usr/lib/udev/rules.d` (debhelper's convention [S22]). Files in `/etc` override files of the
same name and belong to the admin [S2]. All tier 1/2 distros are merged-`/usr`, so `/lib/udev` and `/usr/lib/udev` are
the same directory. Lint in CI with `udevadm verify` (systemd ≥ 254).

### 1.4 Unit skeletons

(U) `/usr/lib/systemd/user/openx3d.service`. The AppImage writes a copy to `~/.config/systemd/user/` with
`ExecStart=` pointing at the AppImage.

```ini
[Unit]
Description=Open X3D Pro remapper
Documentation=https://github.com/kaysond/open-x3d-pro

[Service]
ExecStart=/usr/bin/open-x3d-pro --daemon
Restart=on-failure
NoNewPrivileges=yes
LockPersonality=yes
RestrictAddressFamilies=AF_UNIX AF_NETLINK

[Install]
WantedBy=default.target
```

On first run the GUI enables it with `systemctl --user enable --now openx3d.service`. That needs no root and behaves
the same for every package format. Avoid `systemctl --global enable` in postinst.

(B) `/usr/lib/systemd/system/openx3d-bpf.service` plus `openx3d-bpf.socket`:

```ini
# openx3d-bpf.socket
[Socket]
ListenStream=/run/openx3d/bpf.sock
SocketMode=0666
# The daemon accepts writes only from a peer (SO_PEERCRED) that owns the active seat0 session (logind), mirroring uaccess.

[Install]
WantedBy=sockets.target

# openx3d-bpf.service (started by the socket or by udev SYSTEMD_WANTS)
[Unit]
Description=Open X3D Pro HID-BPF loader
Documentation=https://github.com/kaysond/open-x3d-pro

[Service]
ExecStart=/usr/libexec/openx3d/openx3d-bpfd
# verify: loader works with only these two caps; fallback is User=root with the same bounding set
DynamicUser=yes
AmbientCapabilities=CAP_BPF CAP_PERFMON
CapabilityBoundingSet=CAP_BPF CAP_PERFMON
# BPF attaches by hid_id via sysfs: no device nodes needed
PrivateDevices=yes
DevicePolicy=closed
ProtectSystem=strict
ProtectHome=yes
ProtectKernelModules=yes
ProtectControlGroups=yes
NoNewPrivileges=yes
RestrictAddressFamilies=AF_UNIX AF_NETLINK
SystemCallArchitectures=native
SystemCallFilter=@system-service bpf
```

If the daemon exits, its fds close and the program detaches, so the stick falls back to stock behaviour. That is a
safe failure and means no pinning is needed. Socket semantics (`SocketMode` defaults to 0666; the service has the same
name as the socket) are in [S10].

## 2. Packaging

### 2.1 Tauri v2 targets

| Target | Can | Cannot / caveats |
|---|---|---|
| `deb` | `bundle.linux.deb.files` maps any absolute path (rules, units) [S23]. `preInstallScript`/`postInstallScript`/`preRemoveScript`/`postRemoveScript`, `depends`, `recommends` [S24] | auto-deps `libwebkit2gtk-4.1-0`, `libgtk-3-0`, `libappindicator3-1` when a tray is used [S23]. Debian ships ayatana instead: **verify** the generated `Depends` on Debian 12/13. Debian has **no dpkg trigger for rules.d** (only hwdb, verified in `udev.triggers`), so postinst must reload |
| `rpm` | same keys plus `provides`/`conflicts`/`obsoletes` [S25]. Optional GPG signing via `TAURI_SIGNING_RPM_KEY` [S25] | systemd's rpm file triggers make `%udev_rules_update` a no-op, so a reload happens automatically [S26], but **no `udevadm trigger`**, so already-plugged devices still need it |
| `appimage` | `files` only inside the image under `/usr/` [S28]. Runs anywhere glibc is new enough | **cannot install rules or units.** Users need FUSE2 (`libfuse2t64` on Ubuntu 24.04) or `--appimage-extract-and-run` [S29]. Optional GPG signature that nobody validates automatically [S30] |

Put all Linux config in **`app/src-tauri/tauri.linux.conf.json`**. Tauri merges platform files, so the Windows build is
untouched. It also needs `"resources": []`, because the current `resources/driver/*` glob matches nothing on Linux
(only `.gitkeep` exists), which is expected to fail the bundler (**verify**).

### 2.2 Maintainer scripts (one script serves deb and rpm, which pass different args; keep it idempotent)

```sh
#!/bin/sh
# postinst / %post: apply 70-openx3d.rules now, without reboot or re-login
[ -d /run/systemd/system ] || exit 0   # chroot/container installs
udevadm control --reload-rules || true
modprobe uinput 2>/dev/null || true    # =m on Debian/Arch, built-in elsewhere
udevadm trigger --action=change --sysname-match=uinput || true
udevadm trigger --action=change --subsystem-match=hidraw || true
systemctl daemon-reload || true         # (B) only, plus:
systemctl enable --now openx3d-bpf.socket || true
```

prerm / `%preun`: on removal only (`case "$1" in remove|0)`), run `systemctl disable --now openx3d-bpf.socket
openx3d-bpf.service`. postrm / `%postun`: `udevadm control --reload-rules || true`. Arch runs both reloads through
systemd's alpm hooks `35-systemd-udev-reload.hook` and `30-systemd-daemon-reload-system.hook` [S31].

### 2.3 AppImage first-run helper

The GUI notices `access("/dev/uinput", W_OK)` failing and offers "Install device permissions". Root usually **cannot
read the AppImage's FUSE mount** (no `allow_other`), so the rule text goes over stdin under a single prompt:
`pkexec sh -c 'install -d /etc/udev/rules.d && cat > /etc/udev/rules.d/70-openx3d.rules && udevadm control
--reload-rules && { modprobe uinput; udevadm trigger --action=change --sysname-match=uinput; udevadm trigger
--action=change --subsystem-match=hidraw; }' < rule`. On SteamOS the same call also writes the keep-list (§2.6). pkexec
needs a polkit agent, which KDE and GNOME provide. A Steam Deck has **no sudo password until the user runs `passwd`**.

### 2.4 Flatpak: no for v1

`--device=input` (Flatpak ≥ 1.15.6) only exposes `/dev/input` [S32]. `/dev/uinput` needs `--device=all`; Valve's Steam
Flatpak does this and still relies on host udev rules [S33]. A sandbox cannot install udev rules or units, cannot load
BPF, and runs in its own PID namespace, so host game processes are invisible for per-app profiles [S34]. A later option
is a GUI-only Flatpak talking to a host-installed daemon, which still needs the deb/rpm.

### 2.5 AUR, Nix

* **AUR `open-x3d-pro-bin`**: `source_x86_64=(...amd64.deb)`, extract, depend on `webkit2gtk-4.1 gtk3
  libappindicator-gtk3` [S35]. pacman hooks handle udev and systemd reloads [S31]. The `.install` only prints the
  `systemctl --user enable` hint.
* **Nix/NixOS** (community tier): the derivation installs the rule under `$out/lib/udev/rules.d` and units under
  `$out/lib/systemd/{user,system}`; a module sets `services.udev.packages` and `systemd.packages`. Nix on another
  distro cannot install rules, so it falls back to the AppImage helper.

### 2.6 Steam Deck / SteamOS 3.8 (stable since 2026-06-17, kernel 6.16, then 6.18 in 3.8.28 [S36])

* `/usr` is read-only, and pacman installs do not survive an OS update [S37]. Don't ask users to run
  `steamos-readonly disable`.
* `/etc` is a writable overlay stored under `/var`. **Since 3.6, only allow-listed `/etc` files survive an update**; the
  rest move to `/etc/previous` [S20]. The helper must therefore also write `/etc/atomic-update.conf.d/openx3d.conf`,
  one path or glob per line, as the Nix installer does [S38]:
  `/etc/udev/rules.d/70-openx3d.rules` (plus `/etc/systemd/system/openx3d-bpf.*` for (B)).
* `~/.config/systemd/user` (in `/home`) persists, and user services run in Game Mode, unlike XDG autostart.
* Decky Loader plugins can request root through the `root` flag [S39]. That could be a later Game Mode front-end that
  calls the same helper.
* Alternative to keep-lists: a `systemd-sysext` image in `/var/lib/extensions`. More moving parts; skip for v1.

## 3. HID-BPF distribution (if (B) is chosen)

### 3.1 Facts

* HID-BPF v1 (tracing/fmod_ret plus syscall programs) shipped in 6.3 [S17]. It was **rewritten to struct_ops in 6.11**
  [S18], and attach is now `bpf_map__attach_struct_ops()` with `hid_id` set in the map before load [S15]. Target ≥ 6.11
  only. Supporting both ABIs doubles the BPF code.
* Kconfig: `HID_BPF` depends on `BPF_SYSCALL` and `DYNAMIC_FTRACE_WITH_DIRECT_CALLS` [S19]. **Debian 13 (6.12) has it
  disabled**; it was enabled in linux 6.16.3-1 (sid), and trixie will not get it except possibly through backports
  [S16]. This was confirmed locally: `# CONFIG_HID_BPF is not set` in `config-6.12.107+deb13-amd64`.
* Only **one `hid_rdesc_fixup` program per device** [S15]. If a distro ever ships a fixup for c215, the two collide.

### 3.2 udev-hid-bpf

* Model: hwdb modalias match sets `HID_BPF_*` properties, then a udev rule runs `udev-hid-bpf add` (works like `modprobe`)
  [S40]. Upstream installs to `/usr/local/lib/firmware/hid/bpf`, `/etc/udev/rules.d/81-hid-bpf.rules` and
  `/etc/udev/hwdb.d/81-hid-bpf.hwdb` [S41][S42]. Arch ships `/usr/lib/firmware/hid/bpf/` and `/usr/bin/udev-hid-bpf`
  [S43]. The Debian build searches `/usr/lib/x86_64-linux-gnu/bpf/`. `udev-hid-bpf install X.bpf.o` copies the program
  to `/etc/udev-hid-bpf` and writes a rule into `/etc/udev/rules.d` [S44].
* Config is set **at load time only**, through `UDEV_PROP_*` globals or `--property` [S45]. There is no documented
  runtime map interface, and the pin location is undocumented (**verify** under `/sys/fs/bpf`).
* Packaging (Repology, 2026-10-04 [S46]): Arch 2.3.0; Fedora 43 2.1.0 and 44 2.2.0; Debian forky/sid 2.3.0
  (**not trixie**); Ubuntu 26.04 2.1.0+git and 26.10 2.3.0 (**not 24.04**); CentOS Stream 10 2.1.0. **SteamOS: not
  listed.**

**Verdict:** don't ship through udev-hid-bpf. The gaps are Ubuntu 24.04, Debian 13 and SteamOS, the search paths differ
per distro, there is no runtime config story, and we would depend on its pin layout. Our own loader (§1.4) uses
`libbpf-rs` with `libbpf-sys` built vendored and static (no `libelf1` vs `libelf1t64` dependency split), embeds the
`.bpf.o`, and owns the map. GUI → daemon socket → `bpf_map_update_elem`. Exposing a pinned map to the user is possible
since 5.19 [S12], but it needs a private bpffs mount and chown/chmod management for no gain.

### 3.3 Build toolchain

* clang/LLVM: the runner has 16, 17 and 18.1.3 [S47]. Use clang-18 with `-g -O2 --target=bpf`, as the kernel's
  `drivers/hid/bpf/progs/Makefile` does [S48].
* Skeleton: `libbpf-cargo` `SkeletonBuilder` in `build.rs` generates the Rust skeleton with clang only, so no `bpftool`
  is needed [S49]. Ubuntu noble's `bpftool` is only a virtual package backed by kernel-specific `linux-tools` [S50]. If
  bpftool is ever needed, download the static release from `libbpf/bpftool` (v7.7.0) [S51].
* `vmlinux.h`: **commit a generated copy** (CO-RE relocates at load). The runner kernel (6.17-azure) may lack
  HID-BPF BTF types (**verify** `/sys/kernel/btf/vmlinux` contains `hid_bpf_ops`). Regenerate it from a Fedora or
  virtme-ng kernel with `bpftool btf dump file … format c`.

## 4. CI/CD

### 4.1 Placement

* `build.yml`: new job **`linux`** next to the Windows `build`, on `ubuntu-22.04`. glibc 2.35 is below Debian 12's 2.36,
  and Tauri recommends building on the oldest target base [S23][S28]. It needs no SignPath, so it runs in parallel. Note
  that `nightly.yml`/`release.yml` still gate everything on the SignPath `check`. When `ubuntu-22.04` is retired, move to
  24.04 and drop Debian 12.
* `ci.yml`: `rust-linux` gains the WebKit dev packages once `tauri` stops being `cfg(windows)`-only. Add new lanes
  `linux-e2e` and, for (B), `hid-bpf-vm`.
* `release.yml` `publish`: download both artifacts (`pattern: <prefix>-*-<sha7>`, `merge-multiple`), then regenerate one
  `SHA256SUMS.txt` over all assets with `sha256sum * > SHA256SUMS.txt`. The release-notes template already embeds it.
  Add Linux install lines to the notes.

### 4.2 `linux` bundle job (steps)

1. checkout; `dtolnay/rust-toolchain@stable`; `Swatinem/rust-cache@v2`; `setup-node@v7` (22, npm cache).
2. `apt-get install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev patchelf
   xdg-utils file` [S52][S53], plus `clang-18 llvm-18 libelf-dev zlib1g-dev pkg-config` for (B).
3. `npm ci`, then `npm run tauri -- build --bundles deb,rpm,appimage`. Output goes to the workspace
   `target/release/bundle/{deb,rpm,appimage}/`, located with `cargo metadata`, as build.yml already does.
4. Inspect: `dpkg-deb -I/-c`, `rpm -qip --scripts`, `rpm -qp --requires` [S25]. Fail if the rule or unit files are
   missing.
5. Install smoke in containers (`docker run`): `debian:12`, `debian:13`, `ubuntu:24.04`, `fedora:44`, `archlinux`
   (the `.deb` extracted into the AUR layout). Dependencies must resolve, and scriptlets must exit 0 without systemd
   running (the `/run/systemd/system` guard). Run `udevadm verify` on the rule and `systemd-analyze verify` on the units.
6. Rename to match the Windows naming: `Open-X3D-Pro_<ver>_amd64.deb`, `Open-X3D-Pro-<ver>-1.x86_64.rpm`,
   `Open-X3D-Pro_<ver>_amd64.AppImage`. Write `BUILD_INFO-linux.txt` and upload artifact `<prefix>-linux-<sha7>`
   (14 days).
7. Signing: none required. Optional later steps are rpm GPG (`TAURI_SIGNING_RPM_KEY`) and AppImage GPG (`SIGN=1`)
   [S25][S30]; `SHA256SUMS.txt` is the integrity anchor. An apt/dnf repo would need its own repo signing key (later).
8. Optional: an `ubuntu-24.04-arm` row for aarch64 (free for public repos; linuxdeploy cannot cross-compile [S28]).

### 4.3 `linux-e2e` (ubuntu-24.04, real kernel, passwordless sudo [S54])

1. Print `uname -r` and `grep -E 'UINPUT|UHID|HID_BPF|DEBUG_INFO_BTF' /boot/config-$(uname -r)`.
   `sudo modprobe uinput uhid || true`, then `test -c /dev/uinput`. The runner kernel is 6.17.0-azure [S47]. Ubuntu has
   built uinput in since 10.10 [S55], but **verify** this on the azure flavour; the fallback is the VM lane.
2. There is no logind seat on a runner, so uaccess won't fire. Use `sudo setfacl -m u:$USER:rw /dev/uinput /dev/uhid`
   instead; rule syntax is covered by `udevadm verify`.
3. Fake source: open `/dev/uhid` and create the device with the **real X3D Pro report descriptor** and IDs 046d:c215.
   The kernel's hid-generic then produces the real evdev and hidraw nodes and udev properties, which is more faithful
   than a uinput fake. This is the method the upstream HID selftests use [S56].
4. Start the daemon as the runner user. Wait for the virtual output device, write HID reports into uhid, read the
   output evdev node, and assert the mapping. Assert the grab: a second reader of the source node gets no events.

### 4.4 `hid-bpf-vm` (only for (B))

1. Enable KVM with GitHub's documented udev rule (`99-kvm4all.rules` + `udevadm trigger --name-match=kvm`) [S27].
2. `apt install qemu-system-x86`, then `pip install virtme-ng` [S57].
3. Kernel: `vng --kconfig --config tools/testing/selftests/hid/config` (contains `HID_BPF=y`, `UHID=y`,
   `DEBUG_INFO_BTF=y` [S58]), the same flow as upstream `selftests/hid/vmtest.sh` [S56]. Matrix: 6.12 LTS and the latest
   stable. Cache `bzImage` by version, since a minimal config takes minutes to build on 4 vCPU (**verify** timing).
4. `vng --run bzImage -- ./target/debug/bpf-e2e`: uhid fake stick, load struct_ops, verify evdev output and a map
   update. `vmtest`/`vmtest-action` is an equivalent wrapper [S59].

## 5. Support matrix

| Tier | Distro | Kernel (stock) | (U) uinput daemon | (B) HID-BPF | Package |
|---|---|---|---|---|---|
| 1 | Ubuntu 24.04 LTS | 6.8 GA / 6.17 HWE | yes | HWE only, `HID_BPF` **verify** | deb, AppImage |
| 1 | Ubuntu 26.04 LTS | 6.x (udev-hid-bpf packaged, so HID_BPF likely enabled) | yes | likely | deb, AppImage |
| 1 | Fedora 43 / 44 | current 6.x | yes | yes (udev-hid-bpf packaged) | rpm, AppImage |
| 1 | Arch | rolling | yes | yes (udev-hid-bpf in extra) | AUR -bin, AppImage |
| 1 | SteamOS 3.8 desktop + Game Mode | 6.16 → 6.18 | yes | **verify** `HID_BPF` | AppImage + helper |
| 2 | Debian 13 | 6.12 | yes | **no** (HID_BPF off) unless backports kernel | deb, AppImage |
| 2 | Debian 12 / Ubuntu 22.04 | 6.1 / 5.15 | yes | no (< 6.11) | deb, AppImage (22.04 build host) |
| 2 | Mint 22, Pop!_OS, Bazzite/Nobara, Tumbleweed | follow base | yes | follows kernel | deb/rpm/AppImage |
| 3 | NixOS, non-systemd (no logind uaccess) | – | group fallback | – | community |
| ✗ | Flatpak, Snap | – | – | – | – |

Gates: WebKitGTK **4.1** (Ubuntu 22.04+, Debian 12+ [S28]); glibc ≥ that of the build host (2.35 if built on 22.04);
logind for uaccess; (B) kernel ≥ 6.11 with `CONFIG_HID_BPF=y` (check with `grep HID_BPF /boot/config-$(uname -r)`).

**User docs needed:** per-format install (deb/rpm handle permissions; AppImage uses "Install device permissions"; Steam
Deck needs `passwd` first, Desktop Mode, AppImage, then the helper). Note that uaccess applies **immediately**: replug
the stick if it was already connected. Group fallback (`usermod -aG …`) **requires logging out and back in** and is
not recommended. How to check: `getfacl /dev/uinput` shows `user:<you>:rw-`, and so does `udevadm info -q property -n
/dev/input/by-id/*Extreme_3D*event-joystick`. Logs: `journalctl --user -u openx3d`, and for (B) `journalctl -u
openx3d-bpf`. Uninstall cleanup (AppImage/SteamOS): remove `/etc/udev/rules.d/70-openx3d.rules` and the keep-list file.

## 6. Risks

* **uinput uaccess = keystroke injection by any process of the active user.** Same exposure as Steam's rule, but say so
  in the docs. The stricter alternative is the root daemon of §1.2.
* **Versions:** prerelease tags such as `v0.2.0-rc1` produce deb/rpm versions that sort *after* `0.2.0` or are rejected
  (rpm forbids `-` in Version). Map `-` to `~` for Linux packages or skip prerelease Linux assets (**verify** what Tauri
  does).
* Tauri deb auto-dependency on `libappindicator3-1` may be unsatisfiable on Debian (ayatana only). Override with
  `depends` (**verify**).
* AppImage on newer distros: bundled GTK/WebKit and host mesa/EGL mismatches, plus FUSE2 missing on Ubuntu 24.04 [S29].
* SteamOS: a missing keep-list entry silently removes the rule after an update, and Steam Input may also claim the stick
  in Game Mode (**verify**).
* `ubuntu-22.04` runner retirement forces 24.04, which drops Debian 12 (glibc 2.39).
* (B) availability is uneven (Debian 13, Ubuntu GA kernel, SteamOS unknown). The product therefore needs (U) anyway, and
  (B) at most as an optional fast path.
* A distro or Steam rule that tags the stick differently (e.g. a future systemd game-controller hwdb; PR #22860 was
  closed unmerged [S60]).

## 7. Verify on real distros before committing

1. `udevadm info /dev/input/eventN` for the stick shows `ID_INPUT_JOYSTICK=1` and `TAGS=…:uaccess:` on Ubuntu 24.04,
   Fedora 44, Arch and SteamOS 3.8. Run `getfacl` after the rule plus trigger, without re-login.
2. `/dev/uinput` is a static node on module-built kernels (Debian, Arch), and the ACL is applied after reboot without
   `modules-load.d`.
3. Whether SteamOS already ships a uinput uaccess rule (Steam needs one), whether `deck` is in `input`, whether
   `/etc/atomic-update.conf.d` keeps the rule across a real update, and whether user services run in Game Mode.
4. `CONFIG_HID_BPF` on Ubuntu 24.04 HWE, Ubuntu 26.04, SteamOS 3.8 and the GitHub azure kernel. Check uinput/uhid on
   the azure kernel too.
5. `HID_ID` string of the stick (`udevadm info /sys/bus/hid/devices/0003:046D:C215.*`), and that `SYSTEMD_WANTS` fires
   on coldplug at boot.
6. The (B) loader under `DynamicUser` with only CAP_BPF+CAP_PERFMON on Fedora with SELinux enforcing and Secure Boot
   lockdown.
7. Tauri output: deb `Package:`/filename, `Depends`, rpm Version for prereleases, that the `files` paths land, and that
   scripts run (`dpkg -i` / `dnf install` on clean VMs).
8. AppImage pkexec flow on KDE (SteamOS, Kubuntu) and GNOME (Fedora, Ubuntu), including stdin passing.

## Sources (accessed 2026-10-04)

[S1] systemd `70-uaccess.rules.in` main and v245 tag, https://github.com/systemd/systemd/blob/main/rules.d/70-uaccess.rules.in ; local systemd 257 `/usr/lib/udev/rules.d/{50-udev-default,70-uaccess,73-seat-late}.rules`
[S2] udev(7), https://man7.org/linux/man-pages/man7/udev.7.html
[S3] Valve steam-devices `60-steam-input.rules`, https://github.com/ValveSoftware/steam-devices
[S4] input-remapper service/rules/policy, https://github.com/sezanzeb/input-remapper/tree/main/data
[S5] input-remapper polkit action (`auth_admin_keep`), same repo `data/input-remapper.policy`
[S6] keyd README and `keyd.service.in`, https://github.com/rvaiya/keyd
[S7] NixOS `hardware/uinput.nix`, https://github.com/NixOS/nixpkgs/blob/master/nixos/modules/hardware/uinput.nix
[S8] SC-Controller `69-sc-controller.rules`, https://github.com/Ryochan7/sc-controller
[S9] systemd.resource-control(5) / systemd.exec(5), https://manpages.debian.org/trixie/systemd/systemd.resource-control.5.en.html , https://manpages.debian.org/trixie/systemd/systemd.exec.5.en.html
[S10] systemd.socket(5), https://manpages.debian.org/trixie/systemd/systemd.socket.5.en.html
[S11] kernel `kernel/bpf/syscall.c` (perfmon prog types incl. STRUCT_OPS; CAP_BPF maps), https://github.com/torvalds/linux/blob/master/kernel/bpf/syscall.c
[S12] commit c8644cd0efe7 "bpf: refine kernel.unprivileged_bpf_disabled behaviour" (first in v5.19), https://github.com/torvalds/linux/commit/c8644cd0efe719608ddcb341bcf087d4bc0bf6b8 ; follow-up (6.5) https://kernsec.org/pipermail/linux-security-module-archive/2023-June/037703.html
[S13] kernel `kernel/bpf/inode.c`, https://github.com/torvalds/linux/blob/master/kernel/bpf/inode.c
[S14] systemd `src/shared/mount-setup.c` (bpffs `mode=0700`), https://github.com/systemd/systemd/blob/main/src/shared/mount-setup.c
[S15] HID-BPF kernel docs, https://docs.kernel.org/hid/hid-bpf.html
[S16] Debian bug #1110780 (HID_BPF enabled in 6.16.3-1, not trixie), https://bugs.debian.org/1110780 ; MR https://salsa.debian.org/kernel-team/linux/-/merge_requests/1614
[S17] udev-hid-bpf "How it works" (HID-BPF since 6.3), https://libevdev.pages.freedesktop.org/udev-hid-bpf/how-it-works.html
[S18] "[GIT PULL] HID for 6.11" (struct_ops conversion), https://lkml.iu.edu/hypermail/linux/kernel/2407.2/00508.html
[S19] `drivers/hid/bpf/Kconfig`, https://kernelsources.org/source/xref/linux/drivers/hid/bpf/Kconfig
[S20] Igalia (SteamOS dev), "Keeping your system-wide configuration files intact after updating SteamOS", 2025-02-05, https://blogs.igalia.com/berto/?p=954
[S21] pkexec(1), https://manpages.debian.org/trixie/pkexec/pkexec.1.en.html
[S22] dh_installudev(1), https://manpages.debian.org/trixie/debhelper/dh_installudev.1.en.html
[S23] Tauri v2 Debian, https://v2.tauri.app/distribute/debian/
[S24] Tauri v2 config reference (DebConfig/RpmConfig), https://v2.tauri.app/reference/config/
[S25] Tauri v2 RPM, https://v2.tauri.app/distribute/rpm/
[S26] systemd `src/rpm/macros.systemd.in`, https://github.com/systemd/systemd/blob/main/src/rpm/macros.systemd.in
[S27] GitHub changelog 2024-04-02, KVM on hosted Linux runners, https://github.blog/changelog/2024-04-02-github-actions-hardware-accelerated-android-virtualization-now-available/
[S28] Tauri v2 AppImage, https://v2.tauri.app/distribute/appimage/
[S29] AppImage FUSE troubleshooting, https://docs.appimage.org/user-guide/troubleshooting/fuse.html
[S30] Tauri v2 Linux signing, https://v2.tauri.app/distribute/sign/linux/
[S31] Arch `systemd` package file list (alpm hooks), https://archlinux.org/packages/core/x86_64/systemd/files/
[S32] Flatpak sandbox permissions, https://docs.flatpak.org/en/latest/sandbox-permissions.html
[S33] Flathub Steam manifest (`--device=all`, `/run/udev:ro`), https://github.com/flathub/com.valvesoftware.Steam
[S34] A. Larsson, "The flatpak security model – part 1", https://blogs.gnome.org/alexl/?p=682
[S35] Tauri v2 AUR, https://v2.tauri.app/distribute/aur/
[S36] SteamOS 3.8 release coverage, https://tbreak.com/steamos-3-8-steam-machine-handheld-support/
[S37] Igalia, "More ways to install software in SteamOS", 2024-06-05, https://blogs.igalia.com/berto/2024/06/05/more-ways-to-install-software-in-steamos-distrobox-and-nix/
[S38] DeterminateSystems nix-installer `steam_deck.rs` keep-list, https://github.com/DeterminateSystems/nix-installer/blob/main/src/planner/steam_deck.rs
[S39] Decky plugin dev docs (`root` flag), https://wiki.deckbrew.xyz/plugin-dev/getting-started
[S40] udev-hid-bpf how-it-works (see S17)
[S41] udev-hid-bpf getting started, https://libevdev.pages.freedesktop.org/udev-hid-bpf/getting-started.html
[S42] udev-hid-bpf installing from CI, https://libevdev.pages.freedesktop.org/udev-hid-bpf/installing-from-ci.html
[S43] Arch `udev-hid-bpf` 2.3.0.20260703-2 file list, https://archlinux.org/packages/extra/x86_64/udev-hid-bpf/files/
[S44] udev-hid-bpf(1) (Debian sid), https://manpages.debian.org/unstable/udev-hid-bpf/udev-hid-bpf.1.en.html
[S45] udev-hid-bpf udev properties, https://libevdev.pages.freedesktop.org/udev-hid-bpf/udev-properties.html
[S46] Repology udev-hid-bpf, https://repology.org/project/udev-hid-bpf/versions
[S47] runner-images Ubuntu 24.04 readme (image 20260927.320.1), https://github.com/actions/runner-images/blob/main/images/ubuntu/Ubuntu2404-Readme.md
[S48] kernel `drivers/hid/bpf/progs/Makefile`, https://github.com/torvalds/linux/blob/master/drivers/hid/bpf/progs/Makefile
[S49] libbpf-rs / libbpf-cargo, https://github.com/libbpf/libbpf-rs
[S50] Ubuntu noble `bpftool` (virtual), https://packages.ubuntu.com/noble/bpftool
[S51] bpftool releases, https://github.com/libbpf/bpftool/releases
[S52] Tauri v2 prerequisites, https://v2.tauri.app/start/prerequisites/
[S53] Tauri v2 GitHub pipeline, https://v2.tauri.app/distribute/pipelines/github/
[S54] GitHub-hosted runners reference, https://docs.github.com/en/actions/reference/runners/github-hosted-runners
[S55] Launchpad #584812 (uinput built into Ubuntu kernels), https://bugs.launchpad.net/bugs/584812
[S56] kernel `tools/testing/selftests/hid/vmtest.sh`, https://github.com/torvalds/linux/blob/master/tools/testing/selftests/hid/vmtest.sh
[S57] virtme-ng, https://github.com/arighi/virtme-ng
[S58] kernel `tools/testing/selftests/hid/config`, https://github.com/torvalds/linux/blob/master/tools/testing/selftests/hid/config
[S59] vmtest, https://github.com/danobi/vmtest
[S60] systemd PR #22860 (game-controller uaccess, closed 2024-11-01), https://github.com/systemd/systemd/pull/22860
