; BarePDF Inno Setup Installer Script
; Windows per-user installation with PDF file association registration

#define MyAppName "BarePDF"
#define MyAppVersion "1.0.0"
#define MyAppPublisher "BarePDF Contributors"
#define MyAppURL "https://github.com/barepdf/barepdf"
#define MyAppExeName "BarePDF.exe"
#define MyProgID "BarePDF.Document.1"

[Setup]
AppId={{B3A82379-88F4-4D4D-A815-998A4476B66C}}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={localappdata}\Programs\BarePDF
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\{#MyAppExeName}
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir=..\..\..\target\release\installer
OutputBaseFilename=BarePDF-Setup-x64-v{#MyAppVersion}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ChangesAssociations=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\..\..\target\release\staged\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\..\target\release\staged\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\..\target\release\staged\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: postinstall nowait skipifsilent

[Registry]
; Register ProgID
Root: HKCU; Subkey: "Software\Classes\{#MyProgID}"; ValueType: string; ValueName: ""; ValueData: "PDF Document"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\{#MyProgID}\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"",0"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\{#MyProgID}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""; Flags: uninsdeletekey

; Open With integration
Root: HKCU; Subkey: "Software\Classes\Applications\{#MyAppExeName}"; ValueType: string; ValueName: ""; ValueData: ""; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Applications\{#MyAppExeName}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\.pdf\OpenWithProgids"; ValueType: string; ValueName: "{#MyProgID}"; ValueData: ""; Flags: uninsdeletevalue

; Windows Registered Applications Capabilities
Root: HKCU; Subkey: "Software\BarePDF\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#MyAppName}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\BarePDF\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "Fast, modern, lightweight PDF reader"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\BarePDF\Capabilities\FileAssociations"; ValueType: string; ValueName: ".pdf"; ValueData: "{#MyProgID}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "{#MyAppName}"; ValueData: "Software\BarePDF\Capabilities"; Flags: uninsdeletevalue

[Code]
var
  DefaultAppsPage: TWizardPage;
  RadioYes: TRadioButton;
  RadioNo: TRadioButton;

procedure InitializeWizard();
var
  LabelInfo: TLabel;
begin
  DefaultAppsPage := CreateCustomPage(wpSelectTasks, 'Default PDF Reader', 'Choose whether to set BarePDF as your default PDF reader.');
  
  LabelInfo := TLabel.Create(WizardForm);
  LabelInfo.Parent := DefaultAppsPage.Surface;
  LabelInfo.Left := 0;
  LabelInfo.Top := 0;
  LabelInfo.Width := DefaultAppsPage.SurfaceWidth;
  LabelInfo.Height := 40;
  LabelInfo.Caption := 'Would you like to make BarePDF your default PDF reader?' + #13#10 +
                       'Windows requires you to confirm your selection in Default Apps settings.';
  LabelInfo.WordWrap := True;

  RadioYes := TRadioButton.Create(WizardForm);
  RadioYes.Parent := DefaultAppsPage.Surface;
  RadioYes.Left := 10;
  RadioYes.Top := 50;
  RadioYes.Width := DefaultAppsPage.SurfaceWidth - 20;
  RadioYes.Caption := 'Yes, open Windows Default Apps settings after installation';

  RadioNo := TRadioButton.Create(WizardForm);
  RadioNo.Parent := DefaultAppsPage.Surface;
  RadioNo.Left := 10;
  RadioNo.Top := 80;
  RadioNo.Width := DefaultAppsPage.SurfaceWidth - 20;
  RadioNo.Caption := 'No, keep my current default PDF reader';
  RadioNo.Checked := True;
end;

procedure DeinitializeSetup();
var
  ErrorCode: Integer;
begin
  if (RadioYes <> nil) and RadioYes.Checked and not WizardSilent then
  begin
    if not ShellExec('open', 'ms-settings:defaultapps?registeredAppUser=BarePDF', '', '', SW_SHOWNORMAL, ewNoWait, ErrorCode) then
    begin
      ShellExec('open', 'ms-settings:defaultapps', '', '', SW_SHOWNORMAL, ewNoWait, ErrorCode);
    end;
  end;
end;
