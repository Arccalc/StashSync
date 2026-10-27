# StashSync

StashSync is a small Windows program that reads **your own** ARC Raiders stash and the gear you wear, and sends the item counts to the [ARC Calc](https://arccalc.github.io/) crafting calculator in your browser. You do not have to type your inventory by hand. It can also keep the materials for your craft goals on screen during a raid.

> **Unofficial community tool.** Not affiliated with or endorsed by Embark Studios or Valve. ARC Raiders is a trademark of Embark Studios. StashSync does not modify the game. It does not read or write the game's memory, it does not change game files, and it does not capture network packets. It only asks Embark's servers for your own stash, using the Steam sign-in you already have.

## What's new in 0.3

- **A new window** in the style of the new ARC Calc, in four languages: tabs **Stash**, **Overlay** and **Settings**, a step-by-step first run, and a built-in tour under **?**.
- **Your goals in the window**: how much of the resources for your ARC Calc goals is collected and how much is still to get, with Enough / Partial / None filters.
- **A countdown ring** until the next automatic update; click it or **Update now** to update right away. After each update you see how many items changed.
- **Resizable window**: drag any edge or corner, from 300×360 up to 1100×800. The layout adapts — a compact widget when small, two columns when wide. **Back to 400×680** in Settings restores the default size.
- **Tray**: «—» minimizes to the taskbar, «×» hides the window to the tray and the stash keeps updating (or quits, if you turn that on in Settings). Click the tray icon for quick actions, double-click to open the window, right-click to quit.
- **Overlay tab**: a real-size preview of the in-game panel on a picture of your screen; drag the panel or snap it to a corner, squares or list, direction, size, opacity, the show/hide key and **Hide collected**. Changes apply at once, there is no Save button.
- **Only one copy runs**; starting it again brings the open window to the front.
- Starting with Windows opens StashSync quietly in the tray (or shows the window if you still need to sign in).

## What you need

- Windows 10 or 11.
- [Microsoft Edge WebView2 Runtime](https://go.microsoft.com/fwlink/p/?LinkId=2124703) for the window. Windows 11 already has it, and Windows 10 gets it with Edge updates. If it is missing, the installer and StashSync itself say so and offer Microsoft's download page.
- Steam, signed in to the account that owns the **Steam** edition of ARC Raiders. An Epic-only copy does not work.
- The ARC Calc page open in a browser **on the same computer**.

## Install

1. Download `StashSync-Setup.exe` from [Releases](https://github.com/Arccalc/StashSync/releases/latest).
2. Run it. Windows may show a SmartScreen warning, see [Windows SmartScreen](#windows-smartscreen).
3. The installer works for your Windows user only and needs no administrator rights. It installs to `%LOCALAPPDATA%\StashSync` and adds a Start menu shortcut. A desktop shortcut is optional on the tasks page.

A portable zip with the same files is on the release page too.

## First sync

The window walks you through it the first time:

1. Start Steam and sign in.
2. Close ARC Raiders before the first sign-in. That step starts a short child process which Steam treats as the game, so StashSync refuses to start it while the real game is open. Steam may show you as in-game only while that child process is running. When sign-in finishes, the process exits and Steam clears the in-game state.
3. Press **Sign in**.
4. Open [arccalc.github.io](https://arccalc.github.io/). If your browser asks for access to devices or the local network, click **Allow**.
5. The **Auto Sync** button at the top of the calculator turns online and your stash fills in. The site shows what changed, with **Undo**.

After that you can play normally; StashSync updates the stash by itself. If the sign-in expires, close the game once and sign in again.

## Settings

- **Update stash automatically** — every 1 to 30 minutes, while StashSync is running, even minimized or in the tray.
- **Start with Windows** — starts StashSync in the tray when you sign in to Windows.
- **The × button quits the app** — off by default: × hides the window to the tray.
- **Language** — English, Russian, German or French.
- **Game sign-in** — shows whether the sign-in is saved; **Sign out** deletes it.
- **Window size** — appears when the window is not at its default size.

StashSync does not watch the end of a raid and does not know the moment the game writes the new stash. The next update reads whatever counts Embark returns at that time.

## Goals overlay

Open the **Overlay** tab and press **Show**. The panel lists what is still left to get for your craft goals on the calculator, with progress, and follows the goals as you change them. Use the show/hide key (F8 by default) in the game.

## If the site says "Auto Sync · offline"

- StashSync is not running. Start it.
- Your browser blocked local network access. Click the lock icon in the address bar, open the site settings and allow it, then reload the page.
- The calculator and StashSync are on different computers. They must be on the same one.
- A firewall or antivirus is blocking StashSync. The program makes outbound requests to Embark and to `api.egdata.app`, and it listens on `127.0.0.1:8085`. Allow `StashSync.exe` for those. Do not turn your security software off.
- Sign-in expired. Close the game and sign in again in the StashSync window.

## What it does and does not do

- It reads only **your** stash. It asks Embark's servers for it, using your existing Steam sign-in. You never type your Steam password into StashSync. Sign-in goes through the Steam client that is already open.
- The calculator receives only "item name → count" and the gear you wear, on `ws://127.0.0.1:8085`. That address is your computer only. Sync replaces the stash counts on the site; your craft goals and saved kits are not changed (the worn gear is saved as its own kit, "From the game").
- StashSync listens on `127.0.0.1` only. It accepts `https://arccalc.github.io` and pages served from your own computer (`localhost`, `127.0.0.1` or a local file); other addresses are rejected unless you add them with `--allow-origin <address>`.
- The calculator sends back the list of your goal materials for the overlay. Nothing else.
- It does not read game memory, game files or browser traffic, and it does not capture network packets.
- It does not need administrator rights.

### Network connections

| Where | What is sent | Why |
| --- | --- | --- |
| Embark servers (`auth.embark.net`, `api-gateway.europe.es-pio.net`) | Your sign-in and your stash request | To sign in and read your stash |
| Steam client on your computer | Sign-in request | To sign in with your existing Steam account |
| egdata (`api.egdata.app`) | A request for the public list of game builds. No account data | To find the current game version |
| `127.0.0.1:8085` | Item name → count, worn gear; back: goal materials for the overlay | To fill the calculator and the overlay |

Nothing else leaves your computer.

### What is saved on disk

- Sign-in session: `%LOCALAPPDATA%\ArcCalc`, as a plain JSON file. It is **not encrypted**, so any program running under your Windows account can read it. **Sign out**, deleting the file or uninstalling StashSync signs you out. Never share this file.
- Settings (including the window size and overlay look): `%APPDATA%\ArcCalc`.
- The raw stash is **not** saved during a normal run. A full dump is written only if you start the program with `--dump-raw`.

StashSync loads Valve's `steam_api64.dll` from the same folder as `StashSync.exe`. The build copies that file from the Steamworks redistributable shipped with the `steamworks` library. This repository has no separate written permission from Valve to redistribute it. Check the current Steamworks SDK terms before you publish a build.

## Windows SmartScreen

The installer is not code-signed yet, so Windows may show a blue window saying it protected your PC. This happens because Windows has not seen this file before; it does not by itself mean the file is harmful.

## Uninstall

Remove it in **Windows Settings → Apps**. Uninstall deletes the program folder, the sign-in file in `%LOCALAPPDATA%\ArcCalc`, the settings in `%APPDATA%\ArcCalc` and the start-with-Windows entry.

## Build from source

You do not need Rust to run StashSync. The installer already contains the finished program.

To compile it yourself, on Windows from the project folder:

```
cargo build --release --bin StashSync
```

The built file is `target\release\StashSync.exe`, and the build places `steam_api64.dll` next to it. The window is plain HTML/CSS/JS in `ui/`, shown with WebView2 (`wry` + `tao`). The installer script is `installer/StashSync.iss` (Inno Setup 6).

## Problems and feedback

Open an [issue](https://github.com/Arccalc/StashSync/issues). Please do not post screenshots or logs that contain your sign-in files.
