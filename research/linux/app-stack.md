# Linux support — desktop app stack

Scope: the Tauri app (`app/src-tauri`, `app/src`) on Linux. Device/driver-equivalent work (uinput virtual joystick,
hiding the physical stick) is only covered where it shapes the app. All URLs fetched 2026-10-04 unless a date is
given. "Verify" = needs a real Linux desktop.

## TL;DR

- **Tauri v2 on Linux works** on every target distro (webkit2gtk-4.1 is in Debian 12/13, Ubuntu 22.04+, Fedora, Arch).
  Ship **deb + rpm + AppImage** built in an `ubuntu:22.04` container (glibc 2.35 floor). **Do not ship Flatpak or
  Snap**: the sandbox can't see host processes and has no `/dev/uinput`.
- On Linux there is **no driver**: something has to read the stick, run `x3d_core::pipeline` and write a uinput
  joystick. That loop must not die when the WebKit UI does. **Recommendation: two processes from one crate.** A
  headless `open-x3d-pro --daemon` (systemd user unit) does device I/O, uinput, key chords and switching. The Tauri UI
  talks to it over a Unix socket. No root daemon is needed: udev `uaccess` covers the permissions.
- **Profile switching:** default `matchMode` on Linux = **`running`** (/proc scan; works on every session, including
  Steam Deck Game Mode). `foreground` is implemented **once**, with x11rb on the X server in `$DISPLAY`. That is the
  real X server in X11 sessions, and **XWayland under GNOME, KDE, wlroots and gamescope**, all of which set
  `_NET_ACTIVE_WINDOW` on the XWayland root (verified in their sources). Proton games are X11 clients by default, so
  foreground switching works for them on Wayland without compositor-specific code. GNOME extension and KWin script
  adapters are deferred.
- **Proton matching:** Wine rewrites `/proc/<pid>/cmdline` so argv[0] is the Windows exe path, and sets `comm` to its
  basename (verified in Wine source). The current `switcher::match_profile` already normalises `/` and `\` and falls
  back to the basename, so the profile entry `DCS.exe` matches on both OSes **unchanged**. Add an `exePaths` entry
  form `steam:<appid>`, matched against `SteamAppId`/`SteamGameId` in `/proc/<pid>/environ`.
- **Key chords:** uinput virtual keyboard (`evdev` crate, pure Rust), layout-independent like SendInput scan codes,
  works on X11, Wayland and gamescope.
- **Tray:** none on Linux v1. GNOME has no tray without an extension, Tauri gets no tray click events on Linux, and
  with a resident daemon the UI can be an ordinary window. Add later via tray-icon's new KSNI backend if wanted.
- **Effort:** ≈21 person-days, plus 3–4 days of testing on real desktops (table in §7). A single-process prototype
  gets a working stick in ≈3 days.

## 1. Tauri v2 on Linux today

### 1.1 Versions and webkit2gtk-4.1 availability
Current crates: tauri **2.12.1**, which pulls tray-icon `^0.25` ([docs.rs
features](https://docs.rs/crate/tauri/latest/features)). Tauri v2 needs **webkit2gtk-4.1**: deps are
`libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf` on CI
([pipelines/github](https://v2.tauri.app/distribute/pipelines/github/)), and the
[prerequisites](https://v2.tauri.app/start/prerequisites/) also list `build-essential curl wget file libxdo-dev
libssl-dev libayatana-appindicator3-dev`.

| Distro | webkit2gtk-4.1 (runtime pkg) | Source |
|---|---|---|
| Debian 12 bookworm | 2.50.6-1~deb12u2 (security-updated) | [packages.debian.org](https://packages.debian.org/search?keywords=libwebkit2gtk-4.1-0&searchon=names&suite=all&section=all) |
| Debian 13 trixie | 2.54.0-1~deb13u2 | same |
| Ubuntu 22.04 | 2.50.4-0ubuntu0.22.04.1 | [packages.ubuntu.com](https://packages.ubuntu.com/search?keywords=libwebkit2gtk-4.1-0&searchon=names&suite=all&section=all) |
| Ubuntu 24.04 / 26.04 | 2.52.6 | same |
| Fedora 43/44/45 | 2.52.5 / 2.54.0 / 2.54.0 (`webkit2gtk4.1`) | [packages.fedoraproject.org](https://packages.fedoraproject.org/pkgs/webkitgtk/webkit2gtk4.1/) |
| Fedora 41/42 | EOL by now (not listed any more); both shipped webkit2gtk4.1 | — |
| Arch | 2.52.6-1 (2026-08-19) | [archlinux.org](https://archlinux.org/packages/extra/x86_64/webkit2gtk-4.1/) |
| SteamOS 3.x | Arch-based, read-only `/usr`; whether 4.1 is in the image is **unverified**, so use the AppImage (bundles WebKitGTK) | — |

Debian has **no `libappindicator3-1`** package at all ([search: no
results](https://packages.debian.org/search?keywords=libappindicator3-1&searchon=names&suite=all&section=all)), only
libayatana. Tauri loads the tray library at runtime and prefers ayatana. The v1 guide says the deb depends on
`libayatana-appindicator3-1` unless `TAURI_TRAY=libappindicator3` is set ([v1 tray
guide](https://v1.tauri.app/v1/guides/features/system-tray/)), but the v2 Debian page lists `libappindicator3-1`
([debian](https://v2.tauri.app/distribute/debian/)), which would be uninstallable on Debian. If a tray is ever
shipped, check the generated `control` file and override `bundle.linux.deb.depends`. This is moot if Linux v1 has no
tray (§1.3).

### 1.2 Wayland/X11 quirks
- **NVIDIA/DMABUF:** Tauri's [Linux graphics page](https://v2.tauri.app/develop/debug/linux-graphics/) (updated
  2026-06-15) lists blank/white windows, resize crashes, `AcceleratedSurfaceDMABuf was unable to construct a complete
  framebuffer` and Wayland "Error 71". The cause is WebKitGTK's DMABUF renderer requesting formats NVIDIA lacks. The
  fixes, in order: `nvidia_drm.modeset=1`, `__NV_DISABLE_EXPLICIT_SYNC=1`, `WEBKIT_DISABLE_DMABUF_RENDERER=1`, and as
  a last resort `WEBKIT_DISABLE_COMPOSITING_MODE=1`. Tauri advises setting them in `main()` only when needed.
  WebKitGTK masks the GPU name, so auto-detection is unreliable. **Plan:** a Settings toggle or env passthrough plus
  docs. Do not force it on everyone. With the daemon split, a crashing UI costs nothing in-game.
- **AppImage + EGL:** `EGL_BAD_PARAMETER` white screens on Fedora/Arch and newer Mesa were fixed in **tauri-bundler
  2.10.0 (2026-09-26)**: "Update linuxdeploy and the GTK plugin used for AppImages, fixing `EGL_BAD_PARAMETER`…
  AppImages now support native Wayland instead of forcing X11". The same release also says "Respect an explicitly
  configured `GDK_BACKEND`… while retaining `x11` as the default", and it stopped bundling xdg-open ([release
  notes](https://v2.tauri.app/release/tauri-bundler/v2.10.0/)). Pin bundler ≥ 2.10.0. Before that release the
  community workaround was `LD_PRELOAD=/usr/lib/libwayland-client.so.0` ([example
  report](https://zff.dev/Ampersand/app/issues/193)).
- **Decorations/DPI (unverified, low impact):** GTK3 always draws client-side decorations on Wayland (GNOME-style
  title bar on KDE). Fractional scales render at the next integer and get downscaled, so text is slightly soft. Both
  are cosmetic; verify on KDE at 125%/150%.
- **Focus:** with `xdg-activation`, a second launch's `show_main()` may raise a "ready" notification instead of
  stealing focus on GNOME Wayland. Acceptable.
- **Session trend:** GNOME 50 removed the X11 session (Mutter X11 backend removed in 50.alpha, 2026-01-14;
  [Phoronix](https://www.phoronix.com/news/GNOME-Mutter-Shell-50-Alpha)). Plasma **6.8 (~2026-10-14) drops the X11
  session**; XWayland stays and ">95% of Plasma 6.6 users are on Wayland" ([GamingOnLinux
  2026-06-04](https://www.gamingonlinux.com/2026/06/kde-plasma-waves-goodbye-to-x11-for-plasma-6-8/)). **Design for
  Wayland first.** X11 sessions remain on XFCE/MATE/Cinnamon, Debian 12/13 and Ubuntu 24.04.

### 1.3 Tray
- Backends: tray-icon **0.26.0** has `libappindicator` (default, GTK; needs a running GTK loop) and a new **`ksni`**
  StatusNotifierItem D-Bus backend (added in 0.25.0, GTK-free). 0.26.0: the KSNI backend "no longer fails to build a
  tray icon when no `org.kde.StatusNotifierWatcher` is on the session bus yet"
  ([CHANGELOG](https://github.com/tauri-apps/tray-icon/blob/dev/CHANGELOG.md),
  [docs.rs](https://docs.rs/tray-icon/latest/tray_icon/)). Tauri does not expose `ksni`. Enabling it through a direct
  `tray-icon` dependency (Cargo feature unification; "KSNI takes precedence") is **untested**.
- Semantics on Linux: Tauri says tray events are "Unsupported. The event is not emitted even though the icon is shown
  and will still show a context menu on right click" ([system-tray](https://v2.tauri.app/learn/system-tray/)).
  tray-icon also marks `with_tooltip` (AppIndicator), `with_menu_on_left_click` and `with_menu_on_right_click` as
  unsupported on Linux, while `with_title` is Linux-only
  ([TrayIconBuilder](https://docs.rs/tray-icon/latest/tray_icon/struct.TrayIconBuilder.html)). Effect on `tray.rs`:
  the left-click→`show_main` handler and the tooltip are dead code on Linux. The menu (Open / profile / Pause / Quit)
  still works because AppIndicator opens it on click.
- Hosts: KDE, XFCE, Cinnamon and wlroots bars (waybar etc.) host SNI. **GNOME needs the AppIndicator extension**:
  Ubuntu enables `ubuntu-appindicators` by default; Debian ships it as an optional package
  (`gnome-shell-extension-appindicator` 46 in bookworm, 59 in trixie;
  [packages.debian.org](https://packages.debian.org/search?keywords=gnome-shell-extension-appindicator&searchon=names&suite=all&section=all)).
  Fedora Workstation does not enable it by default (verify). With no host, the icon is simply invisible. An app that
  "starts hidden in tray" then becomes unreachable except by relaunching (single-instance shows the window).
- **Recommendation:** no tray in Linux v1. The daemon is the resident part. The UI is a normal app-grid window, and
  closing it quits only the UI. This removes the libayatana dependency and the GNOME problem. Add a tray later (KSNI)
  if users ask.

### 1.4 Autostart and single instance
- `tauri-plugin-autostart` on Linux delegates to `auto-launch` and passes `$APPIMAGE` as the path when running from an
  AppImage ([plugin src](https://github.com/tauri-apps/plugins-workspace/blob/v2/plugins/autostart/src/lib.rs)).
  auto-launch **0.6.0 (2026-09-11)** writes `~/.config/autostart/<name>.desktop` with args in `Exec=`, and now also
  has a `LinuxLaunchMode::Systemd` option ([docs.rs](https://docs.rs/auto-launch/latest/auto_launch/)) that the plugin
  does not expose. XDG autostart is spec'd at `$XDG_CONFIG_HOME/autostart` with `Hidden`, `OnlyShowIn`/`NotShowIn` and
  `TryExec` ([autostart spec 0.5](https://specifications.freedesktop.org/autostart/latest/)).
- XDG autostart runs **only inside a desktop session** (Plasma, GNOME…), **not in Steam Deck Game Mode** (gamescope
  session; verify). **Recommendation:** Linux `settings.autostart` toggles a **systemd user unit** for the daemon
  (`systemctl --user enable/disable open-x3d-pro.service`, unit shipped in `/usr/lib/systemd/user/`; the AppImage
  writes it to `~/.config/systemd/user/`). Bind it to `graphical-session.target` so `DISPLAY`/`WAYLAND_DISPLAY` are
  already imported ([systemd.special(7)](https://man7.org/linux/man-pages/man7/systemd.special.7.html): "active
  whenever any graphical session is running"). The UI is not autostarted.
- `tauri-plugin-single-instance` on Linux owns the session-bus name `org.{id}.SingleInstance` (here
  `org.dev_kaysond_openx3dpro.SingleInstance`). It works for deb/rpm/AppImage but not by default in Snap/Flatpak
  ([plugin docs](https://v2.tauri.app/plugin/single-instance/)). Keep it for the UI. The daemon gets single-instance
  for free by binding its Unix socket.

### 1.5 Bundles, size, CI
- Targets: **deb**, **rpm**, **AppImage**. The deb depends on `libwebkit2gtk-4.1-0`, `libgtk-3-0` and the appindicator
  lib only if a tray is used ([debian](https://v2.tauri.app/distribute/debian/)). deb and rpm both support `files`,
  `depends`, `pre/postInstallScript`, `pre/postRemoveScript` and `desktopTemplate` ([config
  ref](https://v2.tauri.app/reference/config/)), which is what the udev rule, systemd unit and `udevadm trigger` need.
  rpm signing goes through `TAURI_SIGNING_RPM_KEY` ([rpm](https://v2.tauri.app/distribute/rpm/)).
- Size: deb/rpm **~2–6 MB**, AppImage **70+ MB** (bundles WebKitGTK)
  ([appimage](https://v2.tauri.app/distribute/appimage/)). AppImage `files` destinations must start with `/usr/`, and
  ARM AppImages can't be cross-built.
- Flatpak: the documented path repackages the `.deb` on `org.gnome.Platform` 47 with `--socket=wayland
  --socket=fallback-x11 --device=dri --share=ipc` plus `--talk-name=org.kde.StatusNotifierWatcher` for the tray
  ([flatpak](https://v2.tauri.app/distribute/flatpak/)). It is **not viable for this app** (§3.4).
- glibc: "build… using the oldest base system you intend to support"; Ubuntu 22.04/Debian 12 are the suggested
  baselines ([appimage](https://v2.tauri.app/distribute/appimage/)). CONTRACT §8 pins `ubuntu-24.04` runners (glibc
  2.39), whose output would **not run on Debian 12 (2.36) or Ubuntu 22.04 (2.35)**. **Plan:** keep the `ubuntu-24.04`
  runner but run the bundle job in `container: ubuntu:22.04`. The `ubuntu-22.04` runner image is still GA, but
  `ubuntu-26.04` is now GA too and only two GA versions are kept long-term
  ([runner-images](https://github.com/actions/runner-images)). Install the Tauri deps above plus `libudev-dev` if
  hidapi stays. Set `APPIMAGE_EXTRACT_AND_RUN=1` if linuxdeploy can't use FUSE inside the container (verify).
- `cargo clippy/test` on Linux: if tauri becomes a Linux dependency, the `rust-linux` lane needs the webkit `-dev`
  packages (~1 min apt). Alternatively the daemon/UI split keeps the daemon's Linux build free of webkit.

## 2. Foreground detection on Linux

| Session | Mechanism | PID available? | Status 2026 |
|---|---|---|---|
| X11 (XFCE, MATE, Cinnamon, older GNOME/KDE) | root `PropertyNotify` on `_NET_ACTIVE_WINDOW` → `_NET_WM_PID` / XRes `QueryClientIds` | yes | standard EWMH |
| GNOME Wayland (Mutter) | **XWayland root** `_NET_ACTIVE_WINDOW` for X11 windows. Native Wayland windows: GNOME Shell extension only; `org.gnome.Shell.Eval` needs "unsafe mode" since GNOME 41 ([unsafe-mode-menu](https://github.com/linushdot/unsafe-mode-menu)) | X11 windows yes; extension: `meta_window.get_pid()` | no protocol |
| KDE Plasma 6 Wayland | XWayland root as above; KWin script loaded over D-Bus (`org.kde.KWin /Scripting loadScript`, `workspace.windowActivated`, `window.pid`, `callDBus`; [KWin API](https://develop.kde.org/docs/plasma/kwin/api/), used by [kdotool](https://github.com/jinliu/kdotool)) | yes | works on Wayland |
| wlroots (Sway, Hyprland, labwc, river, niri, Wayfire) | `zwlr_foreign_toplevel_manager_v1` gives app_id, title and `activated`, **no pid** ([wayland.app](https://wayland.app/protocols/wlr-foreign-toplevel-management-unstable-v1)); per-compositor IPC (sway/hyprland/niri) gives pid | IPC only | not on Mutter/KWin/COSMIC/gamescope |
| any | `ext-foreign-toplevel-list-v1` (staging) lists toplevels but has **no activated/focus state**. KWin 6.7+, Sway, Hyprland, niri and COSMIC support it; Mutter does not ([wayland.app](https://wayland.app/protocols/ext-foreign-toplevel-list-v1)). `ext-foreign-toplevel-state` is still a draft MR (wayland-protocols !196; GitLab blocked fetch, title says "Draft") and absent from the protocol index ([wayland.app/protocols](https://wayland.app/protocols/)) | — | not usable |
| gamescope (Deck Game Mode) | sets `_NET_ACTIVE_WINDOW` on its XWayland root ("wine >= 10.0 treats _NET_ACTIVE_WINDOW as foreground") plus `GAMESCOPE_FOCUSED_APP` (Steam appid, CARDINAL) in steam mode ([steamcompmgr.cpp](https://github.com/ValveSoftware/gamescope/blob/master/src/steamcompmgr.cpp)) | via X | works |

**XWayland behaviour, verified in source.** All three major Wayland compositor families keep `_NET_ACTIVE_WINDOW` on
the XWayland root accurate, with a dummy window id when a Wayland-native window has focus:
- **Mutter** `meta_x11_display_update_active_window_hint`: X11 focus → its xwindow; "On Wayland, when a Wayland window
  is focused, indicate that an actual window is focused rather than None", meaning `no_focus_window` ([mutter
  src/x11/meta-x11-display.c](https://github.com/GNOME/mutter/blob/main/src/x11/meta-x11-display.c)).
- **KWin** `RootInfo::setActiveClient`: X11Window → its id; WaylandWindow → `nullFocusWindow()` ([kwin
  src/netinfo.cpp](https://github.com/KDE/kwin/blob/master/src/netinfo.cpp)).
- **wlroots** `xwm_set_focused_window`: X11 surface → `window_id`, else `no_focus_window` ([wlroots
  xwayland/xwm.c](https://gitlab.freedesktop.org/wlroots/wlroots/-/blob/master/xwayland/xwm.c)).

So one x11rb watcher on `$DISPLAY` reports "X11 game X focused" vs "something else focused" in **every** session type.
Proton uses winex11 (XWayland) by default. Valve Proton 10/11 has no Wayland opt-in in the `proton` script or its
README (checked `proton_10.0`, `proton_11.0` and `bleeding-edge`; only Wayland *hacks* such as `WINE_USE_KWIN_HACKS`;
[Proton README](https://github.com/ValveSoftware/Proton/blob/bleeding-edge/README.md)). GE-Proton offers
`PROTON_ENABLE_WAYLAND=1` ([GE-Proton10-1
notes](https://newreleases.io/project/github/GloriousEggroll/proton-ge-custom/release/GE-Proton10-1)). Games using it,
and SDL3 native games on compositors with fifo-v1
([GamingOnLinux](https://www.gamingonlinux.com/2024/03/sdl-3-will-prefer-wayland-over-x11-if-certain-protocols-are-available/)),
are invisible to the X11 path and fall back to "no match → default".

**PID source:** prefer the X-Resource extension (`XResQueryClientIds`, PID from the server's socket credentials,
`x11rb` feature `res`) over `_NET_WM_PID`. For Flatpak Steam the game runs in Flatpak's PID namespace, so a client-set
`_NET_WM_PID` would be the *namespaced* PID (inference; verify). XRes PID comes from the host-side X server. Fallback:
`WM_CLASS` `steam_app_<appid>`, which Proton games set per the [NixOS wiki](https://wiki.nixos.org/wiki/Steam) and
which maps to `steam:<appid>` entries.

**Recommendation (defaults per session):**
- All Linux sessions: default `matchMode = running`. Flight sims run one at a time for hours, the /proc scan has no
  session dependency, and it works under gamescope.
- `foreground` mode: x11rb watcher on `$DISPLAY`. On X11 sessions it covers everything. On Wayland sessions it covers
  X11/XWayland windows (all Proton games by default). A Wayland-native focus is reported as `None` → default profile.
  The UI says "On Wayland, foreground switching sees XWayland/Proton windows only".
- Deferred adapters (only if users ask): KWin script (~1–1.5 d; precedent in
  [keyd-application-mapper](https://github.com/rvaiya/keyd/blob/master/scripts/keyd-application-mapper), which also
  covers wlroots, COSMIC, GNOME and X), GNOME extension (~2–3 d plus per-GNOME-version maintenance; a newly installed
  extension needs a re-login on Wayland and a manual `gnome-extensions enable`,
  [gjs.guide](https://gjs.guide/extensions/development/creating.html)), Hyprland/Sway IPC.
- Note: on GNOME, connecting to `$DISPLAY` starts XWayland on demand and keeps it alive. Only connect while `matchMode
  == foreground`.

## 3. Matching Proton/Wine games

### 3.1 What /proc shows (verified in Wine source, behaviour to verify live)
- Wine's `set_process_name()` calls `prctl(PR_SET_NAME, basename)` after stripping `\` and `/`
  ([dlls/ntdll/unix/env.c](https://github.com/wine-mirror/wine/blob/master/dlls/ntdll/unix/env.c)), so
  **`/proc/<pid>/comm` = `DCS.exe`**, truncated to 15 bytes (TASK_COMM_LEN 16): `MicrosoftFlightSimulator.exe` becomes
  `MicrosoftFlight`.
- `rebuild_argv()` shifts the loader argv[0] out **in place**, and child argv comes from the Windows command line
  (`build_argv(&params->CommandLine…)` in
  [process.c](https://github.com/wine-mirror/wine/blob/master/dlls/ntdll/unix/process.c)). So **`/proc/<pid>/cmdline`
  argv[0] is the Windows command-line argv[0]**, e.g. `C:\Program Files\Eagle Dynamics\DCS World\bin\DCS.exe`,
  `Z:\home\u\…\DCS.exe`, or a Unix path, depending on the launcher (verify). `/proc/<pid>/exe` points at the wine
  (pre)loader and is useless.
- Proton reads/sets `SteamAppId`, `SteamGameId`, `STEAM_COMPAT_APP_ID`, `STEAM_COMPAT_DATA_PATH` and `WINEPREFIX`, and
  starts games through Wine's built-in `c:\windows\system32\steam.exe` ([proton script,
  proton_10.0](https://github.com/ValveSoftware/Proton/blob/proton_10.0/proton)). These are inherited by the game, so
  they are in `/proc/<pid>/environ`. That file is readable for same-uid processes; Yama only restricts attach (verify
  across Flatpak's user namespace).
- Launcher processes (`reaper SteamLaunch AppId=… -- …`, `pressure-vessel-wrap`, `pv-adverb`, `python3 …/proton
  waitforexitandrun /…/DCS.exe`, wine `steam.exe /…/DCS.exe`) carry the game path **as an argument, not argv[0]**.
  Matching argv[0] only avoids false positives.

### 3.2 Namespaces
- **pressure-vessel** (Steam Linux Runtime) shares the host PID namespace. Valve's debugging guide has you take `echo
  $$` *inside* the container and use `/proc/12345/root` *on the host*, and says to "use `ps` to find the process ID of
  any game… inside the container, and then… `ls -l /proc/$game_pid/root/`"
  ([slr-for-game-developers.md](https://gitlab.steamos.cloud/steamrt/steam-runtime-tools/-/blob/main/docs/slr-for-game-developers.md)).
  Game processes are therefore visible with host PIDs.
- **Flatpak Steam** (`com.valvesoftware.Steam`): its sandbox is a child PID namespace. "A process is visible… to the
  processes in each direct ancestor PID namespace", but "processes in a child PID namespace can't see processes in the
  parent" ([pid_namespaces(7)](https://man7.org/linux/man-pages/man7/pid_namespaces.7.html)). A **native** app sees
  Flatpak Steam's games, but a **Flatpak** app cannot see host processes ("No access to processes outside the
  sandbox", and there is no `--share=pid`; `--share` is only `network`/`ipc` per [Flatpak
  permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html)). `--device=input` exposes `/dev/input`
  only, not `/dev/uinput`. **So ship native packages; document that Flatpak/Snap builds are impossible.**

### 3.3 Proposed matching rule (one profile entry for both OSes)
For each process: `name = cmdline[0]`. If cmdline is empty (kernel threads, zombies) skip it; if `cmdline[0]` has no
`.exe`/path, use `readlink(/proc/pid/exe)`. Feed `name` to the **existing** `switcher::match_profile`. `normalize()`
already maps `/` to `\` and lowercases, and the basename fallback then makes `DCS.exe` match `Z:\…\DCS.exe`,
`C:\…\DCS.exe` and `/…/DCS.exe`. Windows full-path entries never full-match on Linux and fall through to the basename,
which is the desired behaviour. **No switcher change is needed for exe names.**
- Edge case (verify): if a launcher passed an unquoted path with spaces, Wine's `build_argv` would split argv[0].
  Fallback: `cmdline` tokens joined with spaces, cut after the first token ending in `.exe`.
- **`steam:<appid>` entries** (new, small): in `exePaths`, an entry `steam:223750` matches a process whose environ has
  `SteamAppId`/`SteamGameId`/`STEAM_COMPAT_APP_ID` equal to it. In foreground mode it also matches
  `GAMESCOPE_FOCUSED_APP` or `WM_CLASS steam_app_223750`. Keeping it inside `exePaths` avoids a §3 type change
  (Windows: `steam:` entries simply never match). The alternative, a `steamAppIds: number[]` field, costs a
  CONTRACT/TS/serde change for no extra power.
- Scan cost: `/proc/[0-9]*/comm` (prefilter: compare the lowercase wanted basename truncated to 15 bytes) → read
  `cmdline` only for hits. ~500 small reads every 2 s, stdlib only.

## 4. Key chords

| Option | X11 | Wayland | gamescope | Needs | Verdict |
|---|---|---|---|---|---|
| **uinput** virtual keyboard ([kernel doc](https://docs.kernel.org/input/uinput.html)) | yes | yes (compositor treats it as a keyboard) | yes | `/dev/uinput` rw | **use** |
| ydotool | yes | yes | yes | its own root/uinput daemon `ydotoold` | pointless wrapper over uinput |
| XTest | yes | XWayland clients only | partial | none | no |
| `zwp_virtual_keyboard_v1` | — | wlroots (KWin privileged only) | no | — | no |
| libei / RemoteDesktop portal | — | GNOME/KDE | no | user consent dialog per session | no |

The daemon already needs uinput for the virtual joystick, so the keyboard is one more uinput device. Crate: [`evdev`
0.13.2](https://docs.rs/evdev/latest/evdev/uinput/index.html), pure Rust (`VirtualDevice::builder().with_keys(..)`),
which also does evdev reading and `EVIOCGRAB`. **`keys.rs` change:** add a Linux `KEY_*` column. For non-extended
set-1 codes below 0x59 the value is identical (KEY_A=30=0x1E, KEY_F12=88=0x58, KEY_102ND=86=0x56). It differs for
E0-extended keys (KEY_KPENTER 96, KEY_RIGHTCTRL 97, KEY_UP 103, KEY_LEFTMETA 125, KEY_COMPOSE 127), F13–F24 (183–194),
and **Pause** (0x45 here but KEY_PAUSE=119, because Linux 69 is KEY_NUMLOCK)
([input-event-codes.h](https://github.com/torvalds/linux/blob/master/include/uapi/linux/input-event-codes.h)).
`Chords` stays as is. Wayland's lack of global hotkeys is irrelevant because we emit and never listen.

## 5. Permissions and first-run UX

- **Joystick evdev node:** already granted to the logged-in seat user by systemd: `SUBSYSTEM=="input",
  ENV{ID_INPUT_JOYSTICK}=="?*", TAG+="uaccess"`
  ([70-uaccess.rules.in](https://github.com/systemd/systemd/blob/main/rules.d/70-uaccess.rules.in)). The kernel binds
  `hid-lg` with `LG_NOGET` for 046d:c215 (hid-lg.c / hid-quirks.c), so there is an evdev node plus a hidraw node.
  **hidraw is not uaccess by default** (only special classes such as 3D mice and hardware wallets).
- **`/dev/uinput`:** root-only by default. Steam's `steam-devices` package grants it with the exact line
  `KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess", OPTIONS+="static_node=uinput"`
  ([60-steam-input.rules](https://github.com/ValveSoftware/steam-devices/blob/master/60-steam-input.rules); Debian
  ships it in `steam-devices`, [filelist](https://packages.debian.org/trixie/all/steam-devices/filelist)), but Arch's
  `steam` package has no udev rules ([files](https://archlinux.org/packages/multilib/x86_64/steam/files/)).
  Precedents: antimicrox ships the same line as `60-antimicrox-uinput.rules`
  ([file](https://github.com/AntiMicroX/antimicrox/blob/master/other/60-antimicrox-uinput.rules)). SC-Controller uses
  uaccess **plus `MODE="0666"`**
  ([69-sc-controller.rules](https://github.com/Ryochan7/sc-controller/blob/python3/scripts/69-sc-controller.rules));
  don't copy the 0666. input-remapper instead runs a **root systemd service** (`sudo systemctl enable --now
  input-remapper`; [README](https://github.com/sezanzeb/input-remapper)) and still has no per-app profiles.
- **Recommendation:** read the stick through **evdev with `EVIOCGRAB`** rather than hidraw. Its node is already
  uaccess, the grab hides the physical stick from games, joydev and Steam, and raw values map 1:1 to `RawReport` (X/Y
  10-bit, Rz/slider 8-bit; `RawReport::to_bytes()` → `pipeline::process`; verify the axis codes and ranges hid-lg
  emits). Then the **only** rule we need is the one-line uinput rule above, shipped as
  `/usr/lib/udev/rules.d/60-open-x3d-pro.rules`. The deb/rpm `postInstallScript` runs `udevadm control --reload &&
  udevadm trigger --subsystem-match=misc --sysname-match=uinput`; the uaccess ACL is then applied to the active
  session without re-login (verify). No polkit, no root daemon. Security trade-off: any process of the logged-in user
  can inject input. This is the same exposure Steam already creates on most gaming PCs. Document it.
- **First run, rule missing** (AppImage, or a distro without the rule): the daemon gets `EACCES` on `/dev/uinput` and
  reports `uinputAccess:false` in state. The UI shows a banner: "Grant joystick/keyboard emulation access", with a
  button that runs `pkexec <appdir>/install-udev-rule.sh`, which writes the rule to `/etc/udev/rules.d/`, reloads and
  triggers. A "copy command" fallback covers systems without pkexec (SteamOS's `deck` user has **no password by
  default**, so pkexec/sudo needs `passwd` first; verify). The polkit-service alternative (input-remapper model) is
  ~3–4 extra days and a root process; not worth it.

## 6. Steam Deck

- **Game Mode** = gamescope session: no tray, no desktop, no XDG autostart (verify). **Desktop Mode** = KDE Plasma 6
  (SteamOS 3.7 shipped Plasma 6.2.5,
  [GamingOnLinux](https://www.gamingonlinux.com/2025/03/steamos-3-7-0-preview-brings-the-beginnings-of-support-for-non-steam-deck-handhelds/));
  the Tauri UI is fine there (AppImage).
- **Immutable OS:** `/usr` is replaced wholesale by updates. `/etc` is a writable overlay stored under `/var`, but
  **since SteamOS 3.6 an update discards `/etc` changes except allow-listed paths** ([Igalia,
  2025-02-05](https://blogs.igalia.com/berto/?p=954)). The default list lives in
  `/usr/lib/rauc/atomic-update-keep.conf`; drop-ins go in `/etc/atomic-update.conf.d/*.conf`, one path glob per line
  (format confirmed by [deck-tailscale](https://github.com/tailscale-dev/deck-tailscale/blob/main/tailscale.sh) and
  [SteamDeck_rEFInd](https://github.com/jlobue10/SteamDeck_rEFInd)). The prompt's assumption "rules in /etc persist"
  is **not reliable**. **Ship `/etc/atomic-update.conf.d/open-x3d-pro.conf` listing
  `/etc/udev/rules.d/60-open-x3d-pro.rules`.** SteamOS ships Steam's udev rules, so uinput may already be uaccess
  there (verify), making our rule a no-op safety net.
- **Shape:** the **systemd user service is the right shape on Deck**: it runs in both modes if bound to a target both
  sessions reach (verify that gamescope-session reaches `graphical-session.target`; otherwise use `default.target` and
  read `DISPLAY` lazily). Game Mode has no display server to show switches, so **default `running` mode**. In
  foreground mode, gamescope's `GAMESCOPE_FOCUSED_APP` gives the appid directly for `steam:` entries.
- **Decky Loader** plugin as the Game Mode UI (Quick Access Menu panel: active profile, pause, manual profile pick).
  It has a React frontend plus a Python backend that can bundle binaries and can request `root` via `plugin.json`
  flags ([decky-plugin-template](https://github.com/SteamDeckHomebrew/decky-plugin-template)). It would just speak the
  daemon socket protocol. ~3–5 d; **defer** until Deck users exist. A joystick on a Deck is niche, while desktop Linux
  is the main audience.

## 7. Architecture: split or not?

**Recommendation: two processes, one crate, no root.**
- `open-x3d-pro --daemon` (systemd user unit, no GTK/WebKit): evdev read+grab → `x3d_core::pipeline` → uinput joystick
  + uinput keyboard chords; profile store, settings, switcher, watcher (/proc + optional X11). Listens on
  `$XDG_RUNTIME_DIR/open-x3d-pro.sock`, speaking newline-delimited JSON `{cmd,args}` → `{ok}|{err}` for the §5
  commands, with the §5 events (`input`, `state`, `profile_switched`) pushed on subscribed connections. Stdlib
  `UnixListener` plus the existing serde_json; no zbus/D-Bus dependency.
- `open-x3d-pro` (Tauri UI): on Linux, `commands.rs` forwards each `#[tauri::command]` to the socket and re-emits
  socket events with `app.emit`. The frontend and the §5 IPC are unchanged.
- **Why not single-process** (Tauri does everything with uinput permissions)? It would be fewer days now (device.rs
  already has the in-app pipeline fallback). But (a) a WebKitGTK crash (NVIDIA/DMABUF, §1.2) or the user quitting the
  UI would kill the virtual joystick mid-flight, which on Windows is the driver's job and never happens; (b) it can't
  run in Deck Game Mode; (c) WebKit stays resident (100+ MB) all day. The daemon *is* the Linux driver. Privilege is
  not the reason: the udev uaccess rule covers both shapes.
- Phasing option: prototype single-process first (≈3 d to a working stick), then split. The modules are already
  Tauri-free apart from the `AppHandle` used for state and emit, so the split is mostly wiring.

### Backend modules → Linux plan

| Module | Linux plan | Runs in | Effort (pd) |
|---|---|---|---|
| `x3d-core` (config, blob, pipeline, hid) | unchanged; pipeline becomes the *primary* path on Linux; blob/feature reports unused | daemon | 0 |
| `profiles.rs` | unchanged; dir = `$XDG_CONFIG_HOME/OpenX3DPro` (CONTRACT §3 path note) | daemon | 0.25 |
| `settings.rs` | unchanged; Linux first-run default `matchMode: running` (cfg default in `Settings::default`, CONTRACT §3) | daemon | 0.25 |
| `switcher.rs` | unchanged logic; add `steam:<appid>` entries (exe string + env/appid input) + tests | daemon | 0.75 |
| `calibration.rs`, `state.rs` | unchanged (+`uinput_access` flag in `AppState`, CONTRACT §5) | daemon | 0.25 |
| `commands.rs` | daemon: same fns behind a socket dispatcher; UI: proxy + event relay | both | 3 |
| `lib.rs` (app wiring) | cfg-split: `--daemon` entry (no Tauri), UI entry (Tauri, no device/watcher); single-instance via socket bind | both | 1.5 |
| `device.rs` | Linux sibling: evdev find by VID/PID (1 s poll as today), `EVIOCGRAB`, RawReport from ABS/BTN, `process()`, uinput joystick out (needs a design decision on output VID/PID/name and axis set, owned by the driver research), emit/coalesce unchanged | daemon | 4 |
| `keys.rs` | add Linux keycode column + uinput keyboard `send()`; `Chords` unchanged | daemon | 1 |
| `watcher.rs` | Linux sibling: /proc scan (comm prefilter → cmdline/environ), x11rb `_NET_ACTIVE_WINDOW` + XRes pid (foreground mode only), `list_windows` = XWayland/X11 `_NET_CLIENT_LIST` ∪ Wine processes from /proc | daemon | 3.5 |
| `tray.rs` | not built on Linux v1 (KSNI tray later: ~1 d) | — | 0 |
| autostart | `systemctl --user enable/disable` of the shipped unit (AppImage writes its own unit); drop `tauri-plugin-autostart` on Linux | daemon | 0.75 |
| packaging | `bundle.targets` + `linux.deb/rpm` (`files`: udev rule, user unit, SteamOS keep-list; postinst reload/trigger), AppImage first-run `pkexec` helper | — | 2 |
| CI | Linux bundle job in `ubuntu:22.04` container on `ubuntu-24.04`; `rust-linux` lane gets webkit deps (or keep daemon lane webkit-free) | — | 1 |
| frontend (`app/src`) | WebKitGTK quirks pass, permission banner, Linux-specific Settings copy (match mode caveat, autostart = background service) | UI | 1.5 |
| CONTRACT/docs | §3 paths + `steam:` entries + Linux default, §5 `uinputAccess`, §6 Linux behaviour, README install | — | 1 |
| **Subtotal** | | | **≈21** |
| Real-desktop testing | GNOME Wayland, KDE Wayland, X11 (XFCE), NVIDIA box, Steam Deck | — | 3–4 |
| Deferred | KWin script 1–1.5 · GNOME extension 2–3 · Decky plugin 3–5 · KSNI tray 1 · Flatpak: not possible | — | — |

## 8. Risks

1. **Wayland foreground blind spot:** native-Wayland games (SDL3 on fifo-v1 compositors, GE-Proton
   `PROTON_ENABLE_WAYLAND=1`, future Proton Wayland default) are invisible to the XWayland watcher. Mitigation:
   `running` default; KWin/GNOME adapters on demand.
2. **WebKitGTK instability** (NVIDIA, AppImage EGL, tauri-bundler 2.10.0 only 8 days old). Mitigation: daemon split,
   documented env toggles, deb/rpm preferred over AppImage.
3. **uinput uaccess** = input injection by any user process. Same as Steam; document it. Alternative: root polkit
   service.
4. **Physical-stick hiding:** if the grab fails, or a game/Wine reads hidraw directly (Proton's winebus uses hidraw
   for some known pads, likely not 046d:c215; verify), games see two sticks. Owned by the driver/device research.
5. **SteamOS:** `/etc` reset on update without the keep-list; no sudo password by default; Game Mode lifecycle of user
   units unverified.
6. **Packaging reach:** no Flatpak/Snap/Flathub. AppImage must be built on the glibc 2.35 floor. CONTRACT §8's
   `ubuntu-24.04` pin needs the container workaround.
7. **Matching edge cases:** `comm` truncation (use cmdline), argv split on unquoted spaces, launchers whose argv[0] is
   a wrapper `.exe` (e.g. game launchers that spawn the sim: profile both names, as on Windows).
8. **Flatpak Steam PIDs:** `_NET_WM_PID` namespaced. Mitigation: XRes; environ readability across Flatpak's user
   namespace unverified.
9. **systemd user unit environment:** `DISPLAY` must be imported before the daemon connects. Bind to
   `graphical-session.target`, reconnect on failure.

## 9. Needs a real Linux desktop to verify

- [ ] GNOME 48/50 Wayland, KDE 6.x Wayland, wlroots (Sway/Hyprland): `xprop -root -spy _NET_ACTIVE_WINDOW` on
      `$DISPLAY` while switching between a Proton game, a native Wayland app and an XWayland app (expect game id /
      dummy id / app id).
- [ ] XRes `QueryClientIds` PID == host PID of a Proton game, with both native Steam and Flatpak Steam; compare with
      `_NET_WM_PID`.
- [ ] A Proton game (DCS or any Steam title): `cat /proc/<pid>/comm`, `tr '\0' '\n' < /proc/<pid>/cmdline`, `grep -z
      SteamAppId /proc/<pid>/environ`; same for Flatpak Steam (environ readable?) and for a non-Steam Wine/Lutris
      launch.
- [ ] uinput keyboard chords reach: a native Wayland app, an XWayland app, a Proton game, a gamescope-hosted game;
      scan-code correctness on a non-US layout.
- [ ] evdev axis/button codes and ranges from hid-lg for 046d:c215; `EVIOCGRAB` hides the stick from
      `jstest`/SDL/Proton; does Steam Input touch the uinput clone?
- [ ] udev rule + `udevadm trigger` grants ACL on `/dev/uinput` without re-login (GNOME, KDE); pkexec helper from the
      AppImage.
- [ ] Tauri UI on NVIDIA proprietary driver + Wayland (blank window? which env var fixes it); KDE fractional scaling;
      GTK CSD look.
- [ ] AppImage built in `ubuntu:22.04` with bundler ≥ 2.10.0 starts on Fedora (current), Arch and SteamOS Desktop Mode
      (Wayland and X11 if offered).
- [ ] Tray (only if re-added): KSNI backend via feature unification; click semantics on KDE vs Ubuntu GNOME.
- [ ] Steam Deck: user unit runs in Game Mode and Desktop Mode across mode switches and reboots;
      `/etc/atomic-update.conf.d` keeps the rule after an OS update; is uinput already uaccess on SteamOS;
      `GAMESCOPE_FOCUSED_APP` readable with gamescope's `--xwayland-count 2`.
