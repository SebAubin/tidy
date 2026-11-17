; Tidy's Windows installer (Inno Setup 6). Built by scripts/package-windows.ps1:
;   iscc /DAppVersion=0.2.0 /DArch=x64|arm64 /DSourceExe=path\to\tidy.exe installer\windows\tidy.iss
; Installs for the current user (no administrator prompt) into %LOCALAPPDATA%\Programs\Tidy,
; with a Start menu shortcut and an uninstaller in Settings > Apps.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef Arch
  #define Arch "x64"
#endif

[Setup]
AppId={{8C2B6E1A-3F4D-4B7E-9A51-6D2E7C9B1F30}
AppName=Tidy
AppVersion={#AppVersion}
AppVerName=Tidy {#AppVersion}
AppPublisher=Sebastien Aubin
AppPublisherURL=https://github.com/SebAubin/tidy
AppSupportURL=https://github.com/SebAubin/tidy/issues
DefaultDirName={localappdata}\Programs\Tidy
DefaultGroupName=Tidy
DisableProgramGroupPage=yes
DisableDirPage=yes
PrivilegesRequired=lowest
OutputBaseFilename=Tidy-{#AppVersion}-windows-{#Arch}-setup
SetupIconFile=..\..\app\assets\icon\Tidy.ico
UninstallDisplayIcon={app}\Tidy.exe
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
#if Arch == "arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif
CloseApplications=yes
; SignPath Foundation's terms: data sent to a service the user didn't pick is described during
; installation, with an option to turn it off (see the Tasks and Code sections).
InfoBeforeFile=privacy.txt
RestartApplications=no

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "Tidy.exe"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Tidy"; Filename: "{app}\Tidy.exe"
Name: "{autodesktop}\Tidy"; Filename: "{app}\Tidy.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked
Name: "telemetry"; Description: "Share anonymous usage statistics (described on the previous page)"

[Run]
Filename: "{app}\Tidy.exe"; Description: "Open Tidy"; Flags: nowait postinstall skipifsilent

[Code]
// Unticked, the statistics task leaves a note Tidy reads on its next launch: it turns the
// setting off there, then deletes the note, so the switch in Settings stays in charge.
procedure CurStepChanged(CurStep: TSetupStep);
var
  Dir: String;
begin
  if (CurStep = ssPostInstall) and not WizardIsTaskSelected('telemetry') then
  begin
    Dir := ExpandConstant('{userappdata}\Tidy');
    ForceDirectories(Dir);
    SaveStringToFile(Dir + '\telemetry-off', '', False);
  end;
end;
