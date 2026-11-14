; PlanoraCAD - Installer (Inno Setup 6)
;
; Was dieses Skript tut: Es packt PlanoraCAD.exe, die Lizenzdateien und die Verknuepfungen in
; ein Setup. Gebaut wird es mit tools\build-installer.ps1, das die Defines AppVersion,
; AppVersionNumeric, SourceExe und LicensesFile setzt. Die Version steht NIE hier.
;
; Die AppId darf nie geaendert werden: Windows erkennt ein Update nur daran als dieselbe App.
;
; Benutzerdaten (%APPDATA%\PlanoraCAD, Dokumente\PlanoraCAD) bleiben bewusst erhalten.
; Der Installer legt dafuer keine Loesch-Regeln an und fasst diese Ordner nie an.

#ifndef AppVersion
  #error AppVersion fehlt. Bitte ueber tools\build-installer.ps1 bauen.
#endif
#ifndef AppVersionNumeric
  #error AppVersionNumeric fehlt. Bitte ueber tools\build-installer.ps1 bauen.
#endif
#ifndef SourceExe
  #error SourceExe fehlt. Bitte ueber tools\build-installer.ps1 bauen.
#endif
#ifndef LicensesFile
  #error LicensesFile fehlt. Bitte ueber tools\build-installer.ps1 bauen.
#endif

[Setup]
AppId={{D42C66D0-6AB0-4D5C-A693-9616105847B8}
AppName=PlanoraCAD
AppVersion={#AppVersion}
VersionInfoVersion={#AppVersionNumeric}
AppPublisher=Dennis Guse
AppPublisherURL=https://github.com/KaelanTesseract/PlanoraCAD
AppSupportURL=https://github.com/KaelanTesseract/PlanoraCAD
DefaultDirName={autopf}\PlanoraCAD
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=commandline dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile=..\LICENSE
SetupIconFile=..\assets\logo\planoracad.ico
UninstallDisplayIcon={app}\PlanoraCAD.exe
OutputDir=..\dist
OutputBaseFilename=planoracad-setup-x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern dynamic
WizardBackColor=#ffffff
WizardBackColorDynamicDark=#181818
CloseApplications=yes
; Kein Neustart ueber den Neustart-Manager: Beim Update beendet sich die App selbst und
; startet ueber [Run] mit /NACHUPDATE=1 neu (sonst liefe sie zweimal).
RestartApplications=no
ShowLanguageDialog=no
DisableProgramGroupPage=yes

[Languages]
; Englisch zuerst: gilt fuer alle Windows-Sprachen ausser Deutsch.
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "de"; MessagesFile: "compiler:Languages\German.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Dirs]
; Ordner fuer weitere Sprachdateien (*.ftl), die das Programm beim Start liest.
Name: "{app}\languages"

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "PlanoraCAD.exe"; Flags: ignoreversion
Source: "{#LicensesFile}"; DestDir: "{app}"; DestName: "THIRD-PARTY-LICENSES.txt"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\PlanoraCAD"; Filename: "{app}\PlanoraCAD.exe"
Name: "{autodesktop}\PlanoraCAD"; Filename: "{app}\PlanoraCAD.exe"; Tasks: desktopicon

[Registry]
Root: HKA; Subkey: "Software\PlanoraCAD"; ValueType: string; ValueName: "InstallMode"; ValueData: "user"; Check: not IsAdminInstallMode; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\PlanoraCAD"; ValueType: string; ValueName: "InstallMode"; ValueData: "all"; Check: IsAdminInstallMode; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\PlanoraCAD"; ValueType: string; ValueName: "InstallPath"; ValueData: "{app}"; Flags: uninsdeletekey

[Run]
; Normale Installation: Haken "PlanoraCAD starten" auf der letzten Seite.
Filename: "{app}\PlanoraCAD.exe"; Description: "{cm:LaunchProgram,PlanoraCAD}"; Flags: nowait postinstall skipifsilent
; Update aus der App (stiller Lauf mit /NACHUPDATE=1): App wieder starten.
Filename: "{app}\PlanoraCAD.exe"; Flags: nowait; Check: IsUpdateRun and not IsAdminInstallMode
; Bei Admin-Lauf ueber explorer.exe, damit die App ohne Admin-Rechte startet.
Filename: "{win}\explorer.exe"; Parameters: """{app}\PlanoraCAD.exe"""; Flags: nowait; Check: IsUpdateRun and IsAdminInstallMode

[Code]
function IsUpdateRun: Boolean;
begin
  Result := ExpandConstant('{param:NACHUPDATE|0}') = '1';
end;

var
  CurrentStep: String;

// Schreibt die Fortschrittsdatei (nur wenn /PROGRESSFILE=<Pfad> uebergeben wurde).
// Die Datei wird jedes Mal ganz neu geschrieben: Zeile 1 percent, Zeile 2 step.
procedure WriteProgress(Percent: Integer; Step: String);
var
  Path: String;
begin
  Path := ExpandConstant('{param:PROGRESSFILE|}');
  if Path = '' then Exit;
  if Percent < 0 then Percent := 0;
  if Percent > 100 then Percent := 100;
  SaveStringToFile(Path, 'percent=' + IntToStr(Percent) + #13#10 + 'step=' + Step, False);
end;

procedure CurInstallProgressChanged(CurProgress, MaxProgress: Integer);
var
  Percent: Integer;
begin
  if MaxProgress > 0 then
    Percent := CurProgress * 100 div MaxProgress
  else
    Percent := 0;
  WriteProgress(Percent, CurrentStep);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  case CurStep of
    ssInstall:
      begin
        CurrentStep := 'install';
        WriteProgress(0, CurrentStep);
      end;
    ssPostInstall:
      begin
        CurrentStep := 'postinstall';
        WriteProgress(100, CurrentStep);
      end;
    ssDone:
      begin
        CurrentStep := 'done';
        WriteProgress(100, CurrentStep);
      end;
  end;
end;

procedure InitializeWizard;
begin
  CurrentStep := 'prepare';
  WriteProgress(0, CurrentStep);
end;
