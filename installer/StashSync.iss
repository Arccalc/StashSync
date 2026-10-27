; Per-user installer. Puts StashSync.exe and steam_api64.dll in the same folder
; and adds a Start menu shortcut. The desktop shortcut is a task the user can skip.
; No administrator rights.

#define MyAppName "StashSync"
#define MyAppVersion "0.3.0"
#define MyAppPublisher "Arccalc"
#define MyAppURL "https://github.com/Arccalc/StashSync"
#define MyAppExeName "StashSync.exe"

[Setup]
AppId={{7C4E9A21-5B18-4F6D-A3C0-8E2D1B6F94A7}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={localappdata}\StashSync
DefaultGroupName=StashSync
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=output
OutputBaseFilename=StashSync-Setup
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ShowLanguageDialog=yes
; English is the preselected language. The dialog still lists the others.
LanguageDetectionMethod=none
UsePreviousLanguage=no
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=yes
RestartApplications=no
AppCopyright=Copyright (C) 2026 {#MyAppPublisher}
VersionInfoVersion={#MyAppVersion}.0
VersionInfoCompany={#MyAppPublisher}
VersionInfoCopyright=Copyright (C) 2026 {#MyAppPublisher}
VersionInfoProductName=StashSync
VersionInfoProductVersion={#MyAppVersion}
VersionInfoDescription=StashSync installer

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "german"; MessagesFile: "compiler:Languages\German.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"
Name: "arabic"; MessagesFile: "compiler:Languages\Arabic.isl"
Name: "armenian"; MessagesFile: "compiler:Languages\Armenian.isl"
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "bulgarian"; MessagesFile: "compiler:Languages\Bulgarian.isl"
Name: "catalan"; MessagesFile: "compiler:Languages\Catalan.isl"
Name: "corsican"; MessagesFile: "compiler:Languages\Corsican.isl"
Name: "czech"; MessagesFile: "compiler:Languages\Czech.isl"
Name: "danish"; MessagesFile: "compiler:Languages\Danish.isl"
Name: "dutch"; MessagesFile: "compiler:Languages\Dutch.isl"
Name: "finnish"; MessagesFile: "compiler:Languages\Finnish.isl"
Name: "hebrew"; MessagesFile: "compiler:Languages\Hebrew.isl"
Name: "hungarian"; MessagesFile: "compiler:Languages\Hungarian.isl"
Name: "italian"; MessagesFile: "compiler:Languages\Italian.isl"
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"
Name: "norwegian"; MessagesFile: "compiler:Languages\Norwegian.isl"
Name: "polish"; MessagesFile: "compiler:Languages\Polish.isl"
Name: "portuguese"; MessagesFile: "compiler:Languages\Portuguese.isl"
Name: "slovak"; MessagesFile: "compiler:Languages\Slovak.isl"
Name: "slovenian"; MessagesFile: "compiler:Languages\Slovenian.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "swedish"; MessagesFile: "compiler:Languages\Swedish.isl"
Name: "tamil"; MessagesFile: "compiler:Languages\Tamil.isl"
Name: "thai"; MessagesFile: "compiler:Languages\Thai.isl"
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"
Name: "ukrainian"; MessagesFile: "compiler:Languages\Ukrainian.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\StashSync.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\steam_api64.dll"; DestDir: "{app}"; Flags: ignoreversion
; Window fonts (SIL Open Font License).
Source: "..\assets\fonts\OFL-Unbounded.txt"; DestDir: "{app}\licenses"; Flags: ignoreversion
Source: "..\assets\fonts\OFL-GolosText.txt"; DestDir: "{app}\licenses"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\StashSync"; Filename: "{app}\{#MyAppExeName}"; AppUserModelID: "Arccalc.StashSync"
Name: "{autodesktop}\StashSync"; Filename: "{app}\{#MyAppExeName}"; AppUserModelID: "Arccalc.StashSync"; Tasks: desktopicon

[UninstallDelete]
Type: filesandordirs; Name: "{app}"
Type: filesandordirs; Name: "{localappdata}\ArcCalc"
Type: filesandordirs; Name: "{userappdata}\ArcCalc"

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[Code]
// The window is a WebView2 page. Windows 11 has the runtime, and Windows 10 gets it with Edge.
// Setup only checks for it and points to Microsoft's page; it downloads and runs nothing itself.
const
  WebView2Id = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';
  WebView2Url = 'https://go.microsoft.com/fwlink/p/?LinkId=2124703';

function HasVersion(Root: Integer; Key: String): Boolean;
var
  Version: String;
begin
  Result := RegQueryStringValue(Root, Key, 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0');
end;

// <Base>\<version>\msedgewebview2.exe, the runtime's own layout.
function HasRuntimeFiles(Base: String): Boolean;
var
  Found: TFindRec;
begin
  Result := False;
  if not FindFirst(Base + '\*', Found) then
    Exit;
  try
    repeat
      if (Found.Attributes and FILE_ATTRIBUTE_DIRECTORY <> 0) and (Found.Name <> '.') and (Found.Name <> '..')
        and FileExists(Base + '\' + Found.Name + '\msedgewebview2.exe') then
      begin
        Result := True;
        Exit;
      end;
    until not FindNext(Found);
  finally
    FindClose(Found);
  end;
end;

// Microsoft's documented registry check, plus the runtime folders in case the registry
// entry is missing: «not found» only when neither has it.
function WebView2Installed: Boolean;
begin
  Result := HasVersion(HKLM, 'SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\' + WebView2Id)
    or HasVersion(HKLM, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\' + WebView2Id)
    or HasVersion(HKCU, 'Software\Microsoft\EdgeUpdate\Clients\' + WebView2Id)
    or HasRuntimeFiles(ExpandConstant('{commonpf32}\Microsoft\EdgeWebView\Application'))
    or HasRuntimeFiles(ExpandConstant('{localappdata}\Microsoft\EdgeWebView\Application'));
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  ErrorCode: Integer;
begin
  if (CurStep <> ssPostInstall) or WebView2Installed then
    Exit;
  if SuppressibleMsgBox('StashSync needs Microsoft Edge WebView2 Runtime, which is missing on this PC. Open the Microsoft download page?',
    mbConfirmation, MB_YESNO, IDNO) = IDYES then
    ShellExec('open', WebView2Url, '', '', SW_SHOWNORMAL, ewNoWait, ErrorCode);
end;

// «Start with Windows» is a value the app writes itself; remove it with the app.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'StashSync');
end;
