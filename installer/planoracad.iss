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

[Messages]
; Deutsch in der Du-Form wie App und Huelle: German.isl spricht mit "Sie" an. Ueberschrieben
; sind die Texte, die den Nutzer ansprechen, im Rueckfall-Assistenten (Willkommen, Lizenz,
; Ordner, Bereit, Installieren, Fertig und ihre Rueckfragen) und in der Deinstallation.
; Platzhalter (%1, %n, [name] ...) bleiben wie im Original. Datei ist UTF-8 ohne BOM.
de.SetupAppRunningError=Das Setup hat bemerkt, dass %1 gerade läuft.%n%nBitte schließe alle Fenster von %1 und klicke auf "OK", um weiterzumachen, oder auf "Abbrechen", um aufzuhören.
de.UninstallAppRunningError=Die Deinstallation hat bemerkt, dass %1 gerade läuft.%n%nBitte schließe alle Fenster von %1 und klicke auf "OK", um weiterzumachen, oder auf "Abbrechen", um aufzuhören.
de.PrivilegesRequiredOverrideInstruction=Bitte wähle, für wen installiert wird
de.PrivilegesRequiredOverrideText1=%1 kann für alle Benutzer (braucht Administratorrechte) oder nur für dich installiert werden.
de.PrivilegesRequiredOverrideText2=%1 kann nur für dich oder für alle Benutzer (braucht Administratorrechte) installiert werden.
de.PrivilegesRequiredOverrideCurrentUser=Installation nur für &mich
de.PrivilegesRequiredOverrideCurrentUserRecommended=Installation nur für &mich (empfohlen)
de.ExitSetupMessage=Das Setup ist noch nicht fertig. Wenn du jetzt aufhörst, wird das Programm nicht installiert.%n%nDu kannst das Setup später noch einmal starten und die Installation dann abschließen.%n%nSetup verlassen?
de.BrowseDialogLabel=Wähle einen Ordner und klicke dann auf "OK".
de.WelcomeLabel2=Dieser Assistent installiert jetzt [name/ver] auf deinem Computer.%n%nAm besten schließt du vorher alle anderen Programme.
de.LicenseLabel=Bitte lies die folgenden wichtigen Informationen, bevor du weitermachst.
de.LicenseLabel3=Bitte lies die folgende Lizenz. Zum Blättern nimm die Bildlaufleiste oder die Taste "Bild ab".
de.SelectDirBrowseLabel=Klicke auf "Weiter", um fortzufahren. Klicke auf "Durchsuchen", wenn du einen anderen Ordner wählen möchtest.
de.InvalidPath=Bitte gib einen vollständigen Pfad mit Laufwerksbuchstaben an, zum Beispiel:%n%nC:\Beispiel%n%noder einen Netzwerkpfad in der Form:%n%n\\Server\Freigabe
de.InvalidDrive=Das Laufwerk oder der Netzwerkpfad existiert nicht oder ist nicht erreichbar. Bitte wähle einen anderen Ordner.
de.DiskSpaceWarning=Das Setup braucht mindestens %1 KB freien Speicherplatz, auf dem gewählten Laufwerk sind aber nur %2 KB frei.%n%nMöchtest du trotzdem weitermachen?
de.DirExists=Den Ordner%n%n%1%n%ngibt es schon. Möchtest du trotzdem in diesen Ordner installieren?
de.SelectTasksLabel2=Wähle, was das Setup bei der Installation von [name] zusätzlich tun soll, und klicke dann auf "Weiter".
de.ReadyLabel1=Das Setup ist jetzt bereit, [name] auf deinem Computer zu installieren.
de.ReadyLabel2a=Klicke auf "Installieren", um zu beginnen, oder auf "Zurück", um deine Einstellungen zu prüfen oder zu ändern.
de.ReadyLabel2b=Klicke auf "Installieren", um zu beginnen.
de.CannotContinue=Das Setup kann nicht weitermachen. Bitte klicke auf "Abbrechen", um es zu beenden.
de.PrepareToInstallNeedsRestart=Das Setup muss deinen Computer neu starten. Starte das Setup danach noch einmal, um die Installation von [name] abzuschließen.%n%nMöchtest du jetzt neu starten?
de.InstallingLabel=Bitte warte, während [name] auf deinem Computer installiert wird.
de.FinishedLabelNoIcons=Das Setup hat [name] auf deinem Computer installiert.
de.FinishedLabel=Das Setup hat [name] auf deinem Computer installiert. Du kannst das Programm über die angelegten Verknüpfungen starten.
de.ClickFinish=Klicke auf "Fertigstellen", um das Setup zu beenden.
de.FinishedRestartLabel=Um die Installation von [name] abzuschließen, muss das Setup deinen Computer neu starten. Möchtest du jetzt neu starten?
de.FinishedRestartMessage=Um die Installation von [name] abzuschließen, muss das Setup deinen Computer neu starten.%n%nMöchtest du jetzt neu starten?
de.SetupAborted=Das Setup konnte nicht abgeschlossen werden.%n%nBitte behebe das Problem und starte das Setup noch einmal.
de.ConfirmUninstall=Möchtest du %1 und alles, was dazugehört, wirklich entfernen?
de.UninstallOnlyOnWin64=Diese Installation lässt sich nur unter 64-Bit-Windows entfernen.
de.UninstallStatusLabel=Bitte warte, während %1 von deinem Computer entfernt wird.
de.UninstalledAll=%1 wurde von deinem Computer entfernt.
de.UninstalledMost=%1 wurde entfernt.%n%nEinige Teile ließen sich nicht entfernen. Du kannst sie von Hand löschen.
de.UninstalledAndNeedsRestart=Um %1 ganz zu entfernen, muss dein Computer neu starten.%n%nMöchtest du jetzt neu starten?

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Dirs]
; Ordner fuer weitere Sprachdateien (*.ftl), die das Programm beim Start liest.
Name: "{app}\languages"
; Leerer Ordner fuer mitgelieferte Plug-ins.
Name: "{app}\plugins"

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
  LastPercent: Integer;
  LastStep: String;

// Schreibt die Fortschrittsdatei (nur wenn /PROGRESSFILE=<Pfad> uebergeben wurde).
// Die Datei wird jedes Mal ganz neu geschrieben: Zeile 1 percent, Zeile 2 step.
procedure WriteProgress(Percent: Integer; Step: String);
var
  Path: String;
begin
  if Percent < 0 then Percent := 0;
  if Percent > 100 then Percent := 100;
  // Nur schreiben, wenn sich Prozent oder Schritt geaendert haben (weniger Dateizugriffe).
  if (Percent = LastPercent) and (Step = LastStep) then Exit;
  Path := ExpandConstant('{param:PROGRESSFILE|}');
  if Path = '' then Exit;
  LastPercent := Percent;
  LastStep := Step;
  SaveStringToFile(Path, 'percent=' + IntToStr(Percent) + #13#10 + 'step=' + Step, False);
end;

procedure CurInstallProgressChanged(CurProgress, MaxProgress: Integer);
var
  Percent: Integer;
begin
  if MaxProgress > 0 then
    // Gleitkomma statt Integer-Multiplikation: CurProgress ist bei grossen Setups in Bytes,
    // CurProgress * 100 wuerde den 32-Bit-Integer ueberlaufen lassen.
    Percent := Trunc(CurProgress * 100.0 / MaxProgress)
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
  LastPercent := -1;
  LastStep := '';
  CurrentStep := 'prepare';
  WriteProgress(0, CurrentStep);
end;
