# StashSync

StashSync is a small Windows program that reads **your own** ARC Raiders stash and sends the item counts to the [ARC Calc](https://arccalc.github.io/) crafting calculator in your browser. You do not have to type your inventory by hand.

> **Unofficial community tool.** Not affiliated with or endorsed by Embark Studios or Valve. ARC Raiders is a trademark of Embark Studios. StashSync does not modify the game. It does not read or write the game's memory, it does not change game files, and it does not capture network packets. It only asks Embark's servers for your own stash, using the Steam sign-in you already have.

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

1. Start Steam and sign in.
2. Close ARC Raiders before the first sign-in. That step starts a short child process which Steam treats as the game, so StashSync refuses to start it while the real game is open. Steam may show you as in-game only while that child process is running. When sign-in finishes, the process exits and Steam clears the in-game state. The saved session is separate and is not what keeps you shown as in-game.
3. Open [arccalc.github.io](https://arccalc.github.io/). If your browser asks for access to devices or the local network, click **Allow**.
4. Start StashSync and press **Sync now**.
5. The badge on the calculator changes to "Auto Sync: Connected" and your inventory fills in.

The sign-in session lasts about a day. While it is alive you can run the game normally. When it ends, close the game once and press **Sync now** again.

## Settings

Open **SETTINGS** in the StashSync window.

- **Auto sync**: checks your stash on a timer, every 1 to 30 minutes. Press **Save** after changing it. With Auto sync off, only **Sync now** updates the inventory.
- **Start with Windows**: adds or removes StashSync from your user startup list.
- **Language**: English, Russian, German or French, the same list as the calculator.
- **Goals overlay**: shows the materials for your craft goals on screen during a raid. Press **Goals overlay** in the StashSync window to turn it on. The list follows the craft goals on the calculator.

StashSync does not watch the end of a raid and does not know the moment the game writes the new stash. The next **Sync now**, or the next auto-sync tick, reads whatever counts Embark returns at that time.

## If the badge says "Auto Sync: offline"

- StashSync is not running. Start it and press **Sync now**.
- Your browser blocked local network access. Click the lock icon in the address bar, open the site settings and allow it, then reload the page.
- The calculator and StashSync are on different computers. They must be on the same one.
- A firewall or antivirus is blocking StashSync. The program makes outbound requests to Embark and to `api.egdata.app`, and it listens on `127.0.0.1:8085`. Allow `StashSync.exe` for those. Testing did not record a specific antivirus window. Do not turn your security software off.
- Sign-in expired. Close the game and press **Sync now** again.

## What it does and does not do

- It reads only **your** stash. It asks Embark's servers for it, using your existing Steam sign-in. You never type your Steam password into StashSync. Sign-in goes through the Steam client that is already open.
- The calculator receives only "item name → count" on `ws://127.0.0.1:8085`. That address is your computer only. Sync replaces the inventory counts; your craft goals and saved loadouts are not changed. StashSync listens on `127.0.0.1` only and accepts connections only from pages loaded from `https://arccalc.github.io`. Other websites are rejected.
- It does not read game memory, game files or browser traffic, and it does not capture network packets.
- It does not need administrator rights.

### Network connections

| Where | What is sent | Why |
| --- | --- | --- |
| Embark servers (`auth.embark.net`, `api-gateway.europe.es-pio.net`) | Your sign-in and your stash request | To sign in and read your stash |
| Steam client on your computer | Sign-in request | To sign in with your existing Steam account |
| egdata (`api.egdata.app`) | A request for the public list of game builds. No account data | To find the current game version |
| `127.0.0.1:8085` | Item name → count | To fill the calculator |

Nothing else leaves your computer.

### What is saved on disk

- Sign-in session: `%LOCALAPPDATA%\ArcCalc`, as a plain JSON file. It is **not encrypted**, so any program running under your Windows account can read it. Deleting the file or uninstalling StashSync signs you out. Never share this file.
- Settings: `%APPDATA%\ArcCalc`.
- The raw stash is **not** saved during a normal run. A full dump is written only if you start the program with `--dump-raw`.

StashSync loads Valve's `steam_api64.dll` from the same folder as `StashSync.exe`. The build copies that file from the Steamworks redistributable shipped with the `steamworks` library. This repository has no separate written permission from Valve to redistribute it. Check the current Steamworks SDK terms before you publish a build.

## Windows SmartScreen

The installer is not code-signed yet, so Windows may show a blue window saying it protected your PC. This happens because Windows has not seen this file before; it does not by itself mean the file is harmful.

## Uninstall

Remove it in **Windows Settings → Apps**. Uninstall deletes the program folder, the sign-in file in `%LOCALAPPDATA%\ArcCalc` and the settings in `%APPDATA%\ArcCalc`.

## Build from source

You do not need Rust to run StashSync. The installer already contains the finished program.

To compile it yourself, on Windows from the project folder:

```
cargo build --release --bin StashSync
```

The built file is `target\release\StashSync.exe`, and the build places `steam_api64.dll` next to it. The installer script is `installer/StashSync.iss` (Inno Setup 6); its output is `installer/output/StashSync-Setup.exe`.

## Problems and feedback

Open an [issue](https://github.com/Arccalc/StashSync/issues). Please do not post screenshots or logs that contain your sign-in files.
