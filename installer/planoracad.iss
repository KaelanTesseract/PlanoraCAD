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
WizardStyle=modern
CloseApplications=yes
RestartApplications=yes
ShowLanguageDialog=no
DisableProgramGroupPage=yes

[Languages]
; Englisch zuerst: gilt fuer alle Windows-Sprachen ausser Deutsch.
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "de"; MessagesFile: "compiler:Languages\German.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

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
